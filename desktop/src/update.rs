//! Aviso de versión nueva. Privacidad: la única petición que sale de la red
//! local es un HEAD a la página `releases/latest` de GitHub, que responde con
//! una redirección a la etiqueta de la última versión publicada. Sin cuerpo,
//! sin identificadores, con un User-Agent fijo, una vez al día, y se apaga en
//! Ajustes. Compartido con el emisor de Linux móvil por `#[path]`: la versión
//! «actual» es la del crate que lo incluye (`CARGO_PKG_VERSION`).

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const REPO: &str = "pepitolas13/PepoMote";
pub const LATEST_URL: &str = "https://github.com/pepitolas13/PepoMote/releases/latest";
/// Fijo y sin versión: GitHub solo ve «un PepoMote», nada más.
const USER_AGENT: &str = "PepoMote-update-check";
/// La primera consulta espera a que la ventana (y el QR) estén en pantalla.
pub const FIRST_DELAY: Duration = Duration::from_secs(3);
pub const CHECK_EVERY: Duration = Duration::from_secs(24 * 3600);
/// Sin red o GitHub caído: se vuelve a intentar en una hora, no cada minuto.
pub const RETRY_AFTER: Duration = Duration::from_secs(3600);
pub const TIMEOUT: Duration = Duration::from_secs(5);
const TICK: Duration = Duration::from_secs(60);

/// Versión `mayor.menor.parche`; se compara numéricamente (1.10 > 1.9) y se
/// guarda en JSON como `[1, 6, 0]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct Version(pub [u32; 3]);

impl Version {
    /// `v1.6.0`, `1.6.0` o `v1.6` (→ 1.6.0). Sufijos (`-beta`) o basura → None.
    pub fn parse(s: &str) -> Option<Version> {
        let s = s.trim();
        let s = s.strip_prefix('v').or_else(|| s.strip_prefix('V')).unwrap_or(s);
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
    let (_, tag) = location.rsplit_once("/tag/")?;
    let tag = tag.split(['?', '#', '/']).next().unwrap_or("");
    Version::parse(tag)
}

/// Página de la release en GitHub (la que se abre desde el aviso).
pub fn release_url(v: &Version) -> String {
    format!("https://github.com/{REPO}/releases/tag/v{v}")
}

/// La versión que hay que anunciar: la última publicada si es mayor que la
/// actual y el usuario no la ocultó.
pub fn pending(current: &Version, latest: Option<Version>, dismissed: Option<Version>) -> Option<Version> {
    latest.filter(|l| l > current && Some(*l) != dismissed)
}

/// ¿Toca consultar? Activado y, o nunca se consultó (`last_check == 0`), o han
/// pasado 24 h desde la última vez (`last_check` y `now` en segundos UNIX; un
/// reloj que va hacia atrás no dispara nada).
pub fn due(enabled: bool, last_check: u64, now: u64) -> bool {
    enabled && (last_check == 0 || now.saturating_sub(last_check) >= CHECK_EVERY.as_secs())
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Un HEAD a `releases/latest` sin seguir la redirección: la etiqueta va en
/// la cabecera `Location`. Las pre-releases y los borradores no cuentan
/// (GitHub no los pone en `latest`).
pub fn fetch_latest(timeout: Duration) -> Result<Version, String> {
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout(timeout)
        .user_agent(USER_AGENT)
        .build();
    let resp = match agent.head(LATEST_URL).call() {
        Ok(r) => r,
        Err(ureq::Error::Status(code, _)) => return Err(format!("GitHub respondió {code}")),
        Err(e) => return Err(e.to_string()),
    };
    let status = resp.status();
    if !(300..400).contains(&status) {
        return Err(format!("respuesta {status} sin redirección"));
    }
    resp.header("Location")
        .and_then(version_from_location)
        .ok_or_else(|| format!("redirección {status} sin etiqueta de versión"))
}

/// Hilo `pmp-update`: cada minuto mira si toca (`due`), consulta y persiste el
/// resultado con `record(ahora, versión)`. `enabled`/`last_check` leen la
/// configuración vigente (el usuario puede apagarlo en caliente).
/// `PEPOMOTE_NO_UPDATE_CHECK` lo desactiva del todo (pruebas e2e).
pub fn spawn(
    enabled: impl Fn() -> bool + Send + 'static,
    last_check: impl Fn() -> u64 + Send + 'static,
    record: impl Fn(u64, Version) + Send + 'static,
    log: impl Fn(String) + Send + 'static,
) {
    if std::env::var_os("PEPOMOTE_NO_UPDATE_CHECK").is_some() {
        return;
    }
    let _ = std::thread::Builder::new().name("pmp-update".into()).spawn(move || {
        std::thread::sleep(FIRST_DELAY);
        let mut failed_at: Option<Instant> = None;
        loop {
            let now = now_secs();
            let retry_ok = failed_at.map_or(true, |t| t.elapsed() >= RETRY_AFTER);
            if retry_ok && due(enabled(), last_check(), now) {
                match fetch_latest(TIMEOUT) {
                    Ok(v) => {
                        failed_at = None;
                        log(format!("Última versión publicada: {v} (esta es {})", Version::current()));
                        record(now, v);
                    }
                    Err(e) => {
                        failed_at = Some(Instant::now());
                        log(format!("Comprobación de versión: {e} (se reintenta en una hora)"));
                    }
                }
            }
            std::thread::sleep(TICK);
        }
    });
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
        for bad in ["v1.6.0-beta", "abc", "", "v", "1", "1.2.3.4", "1..2", "1.x.0"] {
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
        assert_eq!(version_from_location(&format!("{loc}?x=1")), Some(v(1, 5, 0)));
        assert_eq!(version_from_location(&format!("{loc}/")), Some(v(1, 5, 0)));
        assert_eq!(version_from_location("https://github.com/pepitolas13/PepoMote/releases"), None);
        assert_eq!(version_from_location(""), None);
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
        assert_eq!(pending(&cur, Some(v(1, 7, 0)), Some(v(1, 6, 0))), Some(v(1, 7, 0)));
    }

    #[test]
    fn due_respeta_activado_y_24h() {
        assert!(due(true, 0, 1), "nunca consultado: ya");
        assert!(!due(false, 0, 1_000_000_000));
        assert!(!due(true, 1000, 1000 + 86_399));
        assert!(due(true, 1000, 1000 + 86_400));
        // reloj hacia atrás: no dispara
        assert!(!due(true, 5000, 4000));
    }

    #[test]
    fn la_url_de_la_release() {
        assert_eq!(release_url(&v(1, 6, 0)), "https://github.com/pepitolas13/PepoMote/releases/tag/v1.6.0");
    }

    #[test]
    fn la_version_actual_es_la_del_crate() {
        assert!(Version::current() > v(0, 0, 0));
        assert_eq!(Version::current().to_string(), env!("CARGO_PKG_VERSION"));
    }
}
