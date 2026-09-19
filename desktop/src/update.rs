//! Shared receiver/Linux-phone updater: hourly stable manifest checks and an
//! explicit install action. No identifiers are sent. Packages are verified
//! before a separate helper replaces the application and confirms its startup.

use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
#[path = "update_install.rs"]
pub mod install;
#[path = "update_manifest.rs"]
pub mod manifest;
#[path = "update_ui.rs"]
mod ui;
pub use ui::UpdateUi;

static WAKE: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();
pub fn set_wake(wake: impl Fn() + Send + Sync + 'static) {
    let _ = WAKE.set(Box::new(wake));
}

pub const REPO: &str = "pepitolas13/PepoMote";
pub const LATEST_URL: &str = "https://github.com/pepitolas13/PepoMote/releases/latest";
pub const MANIFEST_URL: &str =
    "https://github.com/pepitolas13/PepoMote/releases/latest/download/update.json";
/// Fijo y sin versión: GitHub solo ve «un PepoMote», nada más.
const USER_AGENT: &str = "PepoMote-update-check";
/// La primera consulta espera a que la ventana (y el QR) estén en pantalla.
pub const FIRST_DELAY: Duration = Duration::from_secs(3);
pub const CHECK_EVERY: Duration = Duration::from_secs(3600);
/// Sin red o GitHub caído: se vuelve a intentar en una hora, no cada minuto.
pub const RETRY_AFTER: Duration = Duration::from_secs(3600);
pub const TIMEOUT: Duration = Duration::from_secs(5);
const TICK: Duration = Duration::from_secs(60);

/// Versión `mayor.menor.parche`; se compara numéricamente (1.10 > 1.9) y se
/// guarda en JSON como `[1, 6, 0]`.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct Version(pub [u32; 3]);

impl Version {
    /// `v1.6.0`, `1.6.0` o `v1.6` (→ 1.6.0). Sufijos (`-beta`) o basura → None.
    pub fn parse(s: &str) -> Option<Version> {
        let s = s.trim();
        let s = s
            .strip_prefix('v')
            .or_else(|| s.strip_prefix('V'))
            .unwrap_or(s);
        if s.is_empty() {
            return None;
        }
        let mut parts = [0u32; 3];
        let mut n = 0;
        for p in s.split('.') {
            if n >= 3 || p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            parts[n] = p.parse().ok()?;
            n += 1;
        }
        if n < 2 {
            return None;
        }
        Some(Version(parts))
    }

    /// La versión de esta app (la del crate que incluye este archivo).
    pub fn current() -> Version {
        Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or(Version([0, 0, 0]))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0[0], self.0[1], self.0[2])
    }
}

/// `https://github.com/…/releases/tag/v1.6.0` → 1.6.0.
pub fn version_from_location(location: &str) -> Option<Version> {
    let tag = location.strip_prefix("https://github.com/pepitolas13/PepoMote/releases/tag/")?;
    let tag = tag.split(['?', '#', '/']).next().unwrap_or("");
    Version::parse(tag)
}

/// Página de la release en GitHub (la que se abre desde el aviso).
pub fn release_url(v: &Version) -> String {
    format!("https://github.com/{REPO}/releases/tag/v{v}")
}

/// La versión que hay que anunciar: la última publicada si es mayor que la
/// actual y el usuario no la ocultó.
pub fn pending(
    current: &Version,
    latest: Option<Version>,
    dismissed: Option<Version>,
) -> Option<Version> {
    latest.filter(|l| l > current && Some(*l) != dismissed)
}

/// Startup/hourly checks also recover when the system clock moves backward.
pub fn due(enabled: bool, last_check: u64, now: u64) -> bool {
    enabled
        && (last_check == 0
            || now < last_check
            || now.saturating_sub(last_check) >= CHECK_EVERY.as_secs())
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// GitHub's asset service redirects to its CDN. Each hop must stay on an
/// explicit HTTPS allowlist; the initial repository/tag/name are validated too.
pub fn get_trusted(initial: &str, timeout: Duration) -> Result<ureq::Response, String> {
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout_connect(TIMEOUT)
        .timeout_read(timeout)
        .timeout_write(timeout)
        .user_agent(USER_AGENT)
        .build();
    let first = url::Url::parse(initial).map_err(|e| e.to_string())?;
    if first.host_str() != Some("github.com")
        || first.scheme() != "https"
        || first.query().is_some()
        || first.fragment().is_some()
        || !first.username().is_empty()
        || first.password().is_some()
        || first.port_or_known_default() != Some(443)
        || (initial != MANIFEST_URL && !official_download(&first))
    {
        return Err("Untrusted initial download URL".into());
    }
    let mut current = first.clone();
    let began = Instant::now();
    let total = if initial == MANIFEST_URL {
        Duration::from_secs(60)
    } else {
        Duration::from_secs(15 * 60)
    };
    for _ in 0..6 {
        if began.elapsed() >= total {
            return Err("Download request timed out".into());
        }
        if !trusted_redirect(&current, &first) {
            return Err("Untrusted download redirect".into());
        }
        let response = agent
            .get(current.as_str())
            // ureq propagates this deadline through response headers and the
            // body reader, including chunk framing. Read timeouts alone allow
            // a slowly trickling server to hold the worker indefinitely.
            .timeout(total.saturating_sub(began.elapsed()))
            .set("Accept-Encoding", "identity")
            .call()
            .map_err(|e| e.to_string())?;
        if [301, 302, 303, 307, 308].contains(&response.status()) {
            current = current
                .join(
                    response
                        .header("Location")
                        .ok_or("Missing download redirect")?,
                )
                .map_err(|e| e.to_string())?;
            continue;
        }
        if response.status() != 200
            || response
                .header("Content-Encoding")
                .is_some_and(|s| s != "identity")
        {
            return Err("Unexpected download response".into());
        }
        return Ok(response);
    }
    Err("Too many download redirects".into())
}

fn trusted_redirect(uri: &url::Url, initial: &url::Url) -> bool {
    if uri.scheme() != "https"
        || !uri.username().is_empty()
        || uri.password().is_some()
        || uri.port_or_known_default() != Some(443)
        || uri.fragment().is_some()
    {
        return false;
    }
    match uri.host_str() {
        Some("github.com") => {
            if uri.query().is_some() {
                return false;
            }
            if uri == initial {
                return initial.as_str() == MANIFEST_URL || official_download(uri);
            }
            if initial.as_str() == MANIFEST_URL {
                official_download(uri) && uri.path().ends_with("/update.json")
            } else {
                uri.path() == initial.path()
            }
        }
        Some("release-assets.githubusercontent.com" | "objects.githubusercontent.com") => true,
        _ => false,
    }
}

fn official_download(uri: &url::Url) -> bool {
    let Some(tail) = uri
        .path()
        .strip_prefix("/pepitolas13/PepoMote/releases/download/v")
    else {
        return false;
    };
    let Some((tag, name)) = tail.split_once('/') else {
        return false;
    };
    Version::parse(tag).is_some_and(|v| v.to_string() == tag)
        && (name == "update.json" || manifest::TARGETS.iter().any(|(_, asset)| *asset == name))
}

pub fn fetch_latest(timeout: Duration) -> Result<manifest::Manifest, String> {
    fetch_manifest(timeout, &AtomicBool::new(false))
}
fn fetch_manifest(timeout: Duration, cancel: &AtomicBool) -> Result<manifest::Manifest, String> {
    let began = Instant::now();
    let response = get_trusted(MANIFEST_URL, timeout)?;
    if response
        .header("Content-Length")
        .and_then(|s| s.parse::<u64>().ok())
        .is_some_and(|n| n > manifest::MAX_MANIFEST)
    {
        return Err("Manifest too large".into());
    }
    let mut bytes = Vec::new();
    let mut reader = response.into_reader();
    let mut buf = [0u8; 8192];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        if began.elapsed() > Duration::from_secs(60) {
            return Err("Update check timed out".into());
        }
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        if bytes.len() + n > manifest::MAX_MANIFEST as usize {
            return Err("Manifest too large".into());
        }
        bytes.extend_from_slice(&buf[..n]);
    }
    manifest::Manifest::parse(&bytes)
}

#[derive(Clone, Debug)]
pub enum Phase {
    Idle,
    Checking,
    Available,
    Current,
    Downloading { received: u64, total: u64 },
    Preparing,
    Ready,
    Failed(String),
    Cancelled,
}
impl Phase {
    pub fn busy(&self) -> bool {
        matches!(
            self,
            Self::Checking | Self::Downloading { .. } | Self::Preparing | Self::Ready
        )
    }
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub phase: Phase,
    pub release: Option<manifest::Manifest>,
    pub checked_at: u64,
}
struct State {
    snapshot: Snapshot,
    cancel: Arc<AtomicBool>,
    prepared: Option<install::Plan>,
}
fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(State {
            snapshot: Snapshot {
                phase: Phase::Idle,
                release: None,
                checked_at: 0,
            },
            cancel: Arc::new(AtomicBool::new(false)),
            prepared: None,
        })
    })
}
fn locked() -> std::sync::MutexGuard<'static, State> {
    state().lock().unwrap_or_else(|e| e.into_inner())
}
pub fn snapshot() -> Snapshot {
    locked().snapshot.clone()
}

fn begin_check() -> bool {
    let mut s = locked();
    if s.snapshot.phase.busy() {
        return false;
    }
    s.snapshot.phase = Phase::Checking;
    true
}
fn check() -> Result<Version, String> {
    let result = fetch_latest(Duration::from_secs(15));
    let mut s = locked();
    match result {
        Ok(m) => {
            let v = m.version();
            s.snapshot.phase = if v > Version::current() {
                Phase::Available
            } else {
                Phase::Current
            };
            s.snapshot.release = (v >= Version::current()).then_some(m);
            s.snapshot.checked_at = now_secs();
            Ok(v)
        }
        Err(e) => {
            s.snapshot.phase = Phase::Failed(e.clone());
            Err(e)
        }
    }
}
pub fn request_check() {
    if !begin_check() {
        return;
    }
    if let Err(e) = std::thread::Builder::new()
        .name("pmp-update-check".into())
        .spawn(|| {
            let _ = check();
        })
    {
        locked().snapshot.phase = Phase::Failed(e.to_string());
    }
}

pub fn spawn(
    enabled: impl Fn() -> bool + Send + 'static,
    last_check: impl Fn() -> u64 + Send + 'static,
    record: impl Fn(u64, Version) + Send + 'static,
    log: impl Fn(String) + Send + 'static,
) {
    if std::env::var_os("PEPOMOTE_NO_UPDATE_CHECK").is_some() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("pmp-update".into())
        .spawn(move || {
            std::thread::sleep(FIRST_DELAY);
            let mut first = true;
            let mut failures = 0u32;
            let mut next_retry = Instant::now();
            loop {
                let checked = snapshot().checked_at;
                let last = if checked == 0 { last_check() } else { checked };
                if checked != 0 {
                    first = false;
                }
                if enabled()
                    && Instant::now() >= next_retry
                    && (first || due(true, last, now_secs()))
                    && begin_check()
                {
                    match check() {
                        Ok(v) => {
                            first = false;
                            failures = 0;
                            record(now_secs(), v);
                        }
                        Err(e) => {
                            failures += 1;
                            let delay = match failures {
                                1 => Duration::from_secs(30),
                                2 => Duration::from_secs(120),
                                _ => RETRY_AFTER,
                            };
                            next_retry = Instant::now() + delay;
                            log(format!(
                                "Actualización: {e}; próximo intento en {} s",
                                delay.as_secs()
                            ));
                        }
                    }
                }
                std::thread::sleep(TICK.min(Duration::from_secs(5)));
            }
        });
}

pub fn start_download() {
    let (release, cancel) = {
        let mut s = locked();
        if s.snapshot.phase.busy() {
            return;
        }
        let Some(release) = s
            .snapshot
            .release
            .clone()
            .filter(|m| m.version() > Version::current())
        else {
            return;
        };
        let total = install::Runtime::current()
            .and_then(|r| install::select_target(&r))
            .ok()
            .and_then(|t| release.assets.get(t.key).map(|a| a.size))
            .unwrap_or(0);
        s.cancel = Arc::new(AtomicBool::new(false));
        s.snapshot.phase = Phase::Downloading { received: 0, total };
        (release, s.cancel.clone())
    };
    let result = std::thread::Builder::new()
        .name("pmp-update-install".into())
        .spawn(move || {
            let result = fetch_manifest(Duration::from_secs(5), &cancel)
                .and_then(|fresh| {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("cancelled".into());
                    }
                    if fresh.version != release.version || fresh.assets != release.assets {
                        locked().snapshot.release = Some(fresh);
                        return Err("Release changed. Review the new notes and try again".into());
                    }
                    Ok(fresh)
                })
                .and_then(|fresh| {
                    install::prepare(&fresh, &cancel, |received| {
                        let mut s = locked();
                        if let Phase::Downloading { total, .. } = s.snapshot.phase {
                            s.snapshot.phase = Phase::Downloading { received, total };
                        }
                    })
                })
                .and_then(|plan| {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("cancelled".into());
                    }
                    locked().snapshot.phase = Phase::Preparing;
                    install::start_helper(&plan)?;
                    if cancel.load(Ordering::Relaxed) {
                        let _ = std::fs::write(plan.work.join("abort"), "abort");
                        return Err("cancelled".into());
                    }
                    Ok(plan)
                });
            let mut s = locked();
            match result {
                Ok(plan) => {
                    s.prepared = Some(plan);
                    s.snapshot.phase = Phase::Ready;
                }
                Err(e) => {
                    s.snapshot.phase = if e == "cancelled" {
                        Phase::Cancelled
                    } else {
                        Phase::Failed(e)
                    }
                }
            }
            drop(s);
            if let Some(wake) = WAKE.get() {
                wake();
            }
        });
    if let Err(e) = result {
        locked().snapshot.phase = Phase::Failed(e.to_string());
    }
}
pub fn cancel_download() {
    locked().cancel.store(true, Ordering::Relaxed);
}
pub fn take_prepared() -> Option<install::Plan> {
    locked().prepared.take()
}
pub fn finish_install(plan: &install::Plan) -> Result<(), String> {
    if let Err(e) = install::commit(plan) {
        locked().snapshot.phase = Phase::Failed(e.clone());
        return Err(e);
    }
    // Window close hides into the tray on Windows/macOS. A successful handoff
    // must terminate this process so the helper can replace its executable.
    std::process::exit(0)
}

static OPEN_REQUEST: AtomicBool = AtomicBool::new(false);
pub fn request_open() {
    OPEN_REQUEST.store(true, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: u32, b: u32, c: u32) -> Version {
        Version([a, b, c])
    }

    #[test]
    fn parsea_etiquetas_con_y_sin_v() {
        assert_eq!(Version::parse("v1.6.0"), Some(v(1, 6, 0)));
        assert_eq!(Version::parse("1.6.0"), Some(v(1, 6, 0)));
        assert_eq!(Version::parse(" V1.10.2 "), Some(v(1, 10, 2)));
        assert_eq!(Version::parse("v2.0"), Some(v(2, 0, 0)));
        for bad in [
            "v1.6.0-beta",
            "abc",
            "",
            "v",
            "1",
            "1.2.3.4",
            "1..2",
            "1.x.0",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn compara_numericamente_no_por_texto() {
        assert!(v(1, 10, 0) > v(1, 9, 9));
        assert!(v(2, 0, 0) > v(1, 99, 99));
        assert!(v(1, 6, 1) > v(1, 6, 0));
        assert_eq!(v(1, 6, 0), v(1, 6, 0));
        assert_eq!(v(1, 6, 0).to_string(), "1.6.0");
    }

    #[test]
    fn la_location_de_github_da_la_version() {
        let loc = "https://github.com/pepitolas13/PepoMote/releases/tag/v1.5.0";
        assert_eq!(version_from_location(loc), Some(v(1, 5, 0)));
        assert_eq!(
            version_from_location(&format!("{loc}?x=1")),
            Some(v(1, 5, 0))
        );
        assert_eq!(version_from_location(&format!("{loc}/")), Some(v(1, 5, 0)));
        assert_eq!(
            version_from_location("https://github.com/pepitolas13/PepoMote/releases"),
            None
        );
        assert_eq!(version_from_location(""), None);
        assert_eq!(
            version_from_location("https://evil.example/tag/v9.0.0"),
            None
        );
    }

    #[test]
    fn pending_solo_si_es_mayor_y_no_descartada() {
        let cur = v(1, 5, 0);
        assert_eq!(pending(&cur, None, None), None);
        assert_eq!(pending(&cur, Some(v(1, 5, 0)), None), None);
        assert_eq!(pending(&cur, Some(v(1, 4, 9)), None), None);
        assert_eq!(pending(&cur, Some(v(1, 6, 0)), None), Some(v(1, 6, 0)));
        assert_eq!(pending(&cur, Some(v(1, 6, 0)), Some(v(1, 6, 0))), None);
        // se ocultó la 1.6.0, pero la 1.7.0 es otra: se anuncia
        assert_eq!(
            pending(&cur, Some(v(1, 7, 0)), Some(v(1, 6, 0))),
            Some(v(1, 7, 0))
        );
    }

    #[test]
    fn due_respeta_activado_y_una_hora() {
        assert!(due(true, 0, 1), "nunca consultado: ya");
        assert!(!due(false, 0, 1_000_000_000));
        assert!(!due(true, 1000, 1000 + 3_599));
        assert!(due(true, 1000, 1000 + 3_600));
        // El reloj puede corregirse tras arrancar sin red: se comprueba y se
        // guarda la nueva hora, sin quedar bloqueado por una fecha futura.
        assert!(due(true, 5000, 4000));
    }

    #[test]
    fn la_url_de_la_release() {
        assert_eq!(
            release_url(&v(1, 6, 0)),
            "https://github.com/pepitolas13/PepoMote/releases/tag/v1.6.0"
        );
    }

    #[test]
    fn la_version_actual_es_la_del_crate() {
        assert!(Version::current() > v(0, 0, 0));
        assert_eq!(Version::current().to_string(), env!("CARGO_PKG_VERSION"));
    }
    #[test]
    fn redirects_stay_on_github_https_and_the_exact_asset() {
        let initial = url::Url::parse(
            "https://github.com/pepitolas13/PepoMote/releases/download/v2.0.0/PepoMote.exe",
        )
        .unwrap();
        assert!(trusted_redirect(&initial, &initial));
        assert!(trusted_redirect(
            &url::Url::parse("https://release-assets.githubusercontent.com/file?signature=value")
                .unwrap(),
            &initial
        ));
        for bad in [
            "http://github.com/pepitolas13/PepoMote/releases/download/v2.0.0/PepoMote.exe",
            "https://github.com/pepitolas13/PepoMote/releases/download/v2.0.1/PepoMote.exe",
            "https://github.com/pepitolas13/Other/releases/download/v2.0.0/PepoMote.exe",
            "https://evil.example/file",
            "https://user@objects.githubusercontent.com/file",
        ] {
            assert!(
                !trusted_redirect(&url::Url::parse(bad).unwrap(), &initial),
                "{bad}"
            );
        }
    }
}
