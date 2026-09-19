//! Adjacent staging; a separate copy of this binary performs the replacement.
//! The running application and its settings are never modified by downloads.
use super::manifest::{Asset, Manifest};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct Runtime {
    pub os: String,
    pub arch: String,
    pub mobile: bool,
    pub musl: bool,
    pub executable: PathBuf,
    pub appimage: Option<PathBuf>,
    pub managed: bool,
}

#[derive(Clone, Debug)]
pub struct Target {
    pub key: &'static str,
    pub destination: PathBuf,
    pub bundle: bool,
}

pub fn select_target(r: &Runtime) -> Result<Target, String> {
    if r.managed {
        return Err("managed".into());
    }
    let key = match (
        r.os.as_str(),
        r.arch.as_str(),
        r.mobile,
        r.musl,
        r.appimage.is_some(),
    ) {
        ("windows", "x86_64", false, _, false) => "windows-x86_64",
        ("linux", "x86_64", false, false, false) => "linux-x86_64",
        ("linux", "x86_64", false, false, true) => "linux-x86_64-appimage",
        ("linux", "aarch64", true, false, false) => "mobile-linux-aarch64",
        ("linux", "aarch64", true, true, false) => "mobile-linux-aarch64-musl",
        ("linux", "aarch64", true, false, true) => "mobile-linux-aarch64-appimage",
        ("macos", "aarch64", false, _, false) => "macos-aarch64",
        _ => return Err("unsupported".into()),
    };
    let bundle = r.os == "macos";
    let destination = if bundle {
        r.executable
            .ancestors()
            .find(|p| p.extension().is_some_and(|s| s == "app"))
            .ok_or("bundle")?
            .to_owned()
    } else {
        r.appimage.as_ref().unwrap_or(&r.executable).clone()
    };
    Ok(Target {
        key,
        destination,
        bundle,
    })
}

impl Runtime {
    pub fn current() -> Result<Self, String> {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let appimage = std::env::var_os("APPIMAGE").map(PathBuf::from);
        let destination = appimage.as_ref().unwrap_or(&executable);
        let managed = std::env::var_os("FLATPAK_ID").is_some()
            || std::env::var_os("SNAP").is_some()
            || Path::new("/.flatpak-info").exists()
            || system_path(destination);
        Ok(Self {
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            mobile: env!("CARGO_PKG_NAME") == "pepomote-mobile",
            musl: cfg!(target_env = "musl"),
            executable,
            appimage,
            managed,
        })
    }
}

fn system_path(p: &Path) -> bool {
    #[cfg(windows)]
    {
        let normalized = p.to_string_lossy().replace('/', "\\").to_lowercase();
        let normalized = normalized.strip_prefix("\\\\?\\").unwrap_or(&normalized);
        [
            "ProgramFiles",
            "ProgramFiles(x86)",
            "ProgramW6432",
            "SystemRoot",
        ]
        .iter()
        .filter_map(std::env::var_os)
        .any(|root| {
            normalized.starts_with(
                &(PathBuf::from(root)
                    .to_string_lossy()
                    .replace('/', "\\")
                    .to_lowercase()
                    + "\\"),
            )
        })
    }
    #[cfg(not(windows))]
    {
        [
            "/usr",
            "/bin",
            "/sbin",
            "/opt",
            "/nix",
            "/snap",
            "/var/lib/flatpak",
            "/Volumes",
        ]
        .iter()
        .any(|base| p.starts_with(base))
    }
}

pub fn verify_stream(
    mut reader: impl Read,
    mut output: impl Write,
    asset: &Asset,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> Result<(), String> {
    if asset.size == 0 || asset.size > super::manifest::MAX_PACKAGE {
        return Err("Invalid package size".into());
    }
    let mut hash = Sha256::new();
    let mut received = 0u64;
    let mut buf = [0u8; 64 * 1024];
    let began = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        if began.elapsed() > Duration::from_secs(15 * 60) {
            return Err("Download timed out".into());
        }
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        received += n as u64;
        if received > asset.size {
            return Err("Package exceeds expected size".into());
        }
        hash.update(&buf[..n]);
        output.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        progress(received);
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    if received != asset.size
        || format!("{:x}", hash.finalize()) != asset.sha256.to_ascii_lowercase()
    {
        return Err("Package verification failed (size / SHA-256)".into());
    }
    output.flush().map_err(|e| e.to_string())
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub work: PathBuf,
    pub destination: PathBuf,
    pub staged: PathBuf,
    pub bundle: bool,
    pub asset: Asset,
    pub version: String,
    pub parent_pid: u32,
    pub launch_args: Vec<String>,
    pub original_sha256: String,
}

fn reject_links(path: &Path) -> Result<(), String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("Unsafe installation path".into());
    }
    for p in path.ancestors() {
        match fs::symlink_metadata(p) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err("Symbolic links are not supported by the updater".into())
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

fn durable_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = fs::File::create(path).map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())
}

fn executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    let _ = path;
    Ok(())
}

fn backup(plan: &Plan) -> PathBuf {
    plan.work.join(if plan.bundle {
        "previous.app"
    } else {
        "previous"
    })
}

fn installed_executable(plan: &Plan) -> PathBuf {
    if plan.bundle {
        plan.destination.join("Contents/MacOS/PepoMote")
    } else {
        plan.destination.clone()
    }
}
fn file_hash(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut bytes).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&bytes[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

#[cfg(target_os = "macos")]
fn swap_bundles(a: &Path, b: &Path) -> Result<(), String> {
    use std::os::unix::ffi::OsStrExt;
    let a = std::ffi::CString::new(a.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    let b = std::ffi::CString::new(b.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    if unsafe { libc::renamex_np(a.as_ptr(), b.as_ptr(), libc::RENAME_SWAP) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}

#[cfg(windows)]
fn atomic_replace(
    destination: &Path,
    source: &Path,
    previous: Option<&Path>,
) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn ReplaceFileW(
            replaced: *const u16,
            replacement: *const u16,
            backup: *const u16,
            flags: u32,
            exclude: *mut std::ffi::c_void,
            reserved: *mut std::ffi::c_void,
        ) -> i32;
    }
    let wide = |p: &Path| {
        p.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let d = wide(destination);
    let s = wide(source);
    let b = previous.map(wide);
    for n in 0..50 {
        if unsafe {
            ReplaceFileW(
                d.as_ptr(),
                s.as_ptr(),
                b.as_ref().map_or(std::ptr::null(), |v| v.as_ptr()),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } != 0
        {
            return Ok(());
        }
        if n == 49 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    unreachable!()
}

fn replace(plan: &Plan) -> Result<(), String> {
    reject_links(&plan.destination)?;
    reject_links(&plan.staged)?;
    if !plan.staged.exists() || !plan.destination.exists() || backup(plan).exists() {
        return Err("Invalid staged installation or existing backup".into());
    }
    durable_write(&plan.work.join("journal"), b"prepared")?;
    if plan.bundle {
        #[cfg(target_os = "macos")]
        {
            // Atomic directory exchange leaves the main .app path launchable
            // even if power fails between the swap and naming the backup.
            swap_bundles(&plan.staged, &plan.destination)?;
            if let Err(e) = fs::rename(&plan.staged, backup(plan)) {
                swap_bundles(&plan.staged, &plan.destination)?;
                return Err(e.to_string());
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            fs::rename(&plan.destination, backup(plan)).map_err(|e| e.to_string())?;
            if let Err(e) = fs::rename(&plan.staged, &plan.destination) {
                let restored = fs::rename(backup(plan), &plan.destination);
                return Err(format!("{e}; restore: {restored:?}"));
            }
        }
    } else {
        executable(&plan.staged)?;
        #[cfg(windows)]
        atomic_replace(&plan.destination, &plan.staged, Some(&backup(plan)))?;
        #[cfg(not(windows))]
        {
            fs::copy(&plan.destination, backup(plan)).map_err(|e| e.to_string())?;
            fs::File::open(backup(plan))
                .and_then(|f| f.sync_all())
                .map_err(|e| e.to_string())?;
            fs::rename(&plan.staged, &plan.destination).map_err(|e| e.to_string())?;
        }
    }
    durable_write(&plan.work.join("journal"), b"replaced")
}

fn rollback(plan: &Plan) -> Result<(), String> {
    let previous = backup(plan);
    reject_links(&plan.destination)?;
    reject_links(&previous)?;
    let backup_executable = if plan.bundle {
        previous.join("Contents/MacOS/PepoMote")
    } else {
        previous.clone()
    };
    if file_hash(&backup_executable).ok().as_ref() != Some(&plan.original_sha256) {
        return if file_hash(&installed_executable(plan)).ok().as_ref()
            == Some(&plan.original_sha256)
        {
            Ok(())
        } else {
            Err("Previous installation is missing or damaged; recovery is required".into())
        };
    }
    if plan.bundle {
        #[cfg(target_os = "macos")]
        {
            if plan.destination.exists() {
                swap_bundles(&previous, &plan.destination)?;
            } else {
                fs::rename(previous, &plan.destination).map_err(|e| e.to_string())?;
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            if plan.destination.exists() {
                fs::rename(&plan.destination, plan.work.join("failed.app"))
                    .map_err(|e| e.to_string())?;
            }
            fs::rename(previous, &plan.destination).map_err(|e| e.to_string())?;
        }
    } else {
        #[cfg(windows)]
        {
            if plan.destination.exists() {
                atomic_replace(&plan.destination, &previous, None)?;
            } else {
                fs::rename(previous, &plan.destination).map_err(|e| e.to_string())?;
            }
        }
        #[cfg(not(windows))]
        fs::rename(previous, &plan.destination).map_err(|e| e.to_string())?;
    }
    durable_write(&plan.work.join("journal"), b"rolled-back")
}

fn command(exe: &Path) -> Command {
    let mut c = Command::new(exe);
    c.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    c.env_remove("PEPOMOTE_UPDATE_HEALTH");
    // Do not leak an old AppImage's mounted libraries into the new version.
    if std::env::var_os("APPDIR").is_some() || std::env::var_os("APPIMAGE").is_some() {
        for key in [
            "APPIMAGE",
            "APPDIR",
            "ARGV0",
            "OWD",
            "LD_LIBRARY_PATH",
            "LD_PRELOAD",
        ] {
            c.env_remove(key);
        }
        if let (Some(mount), Some(path)) = (std::env::var_os("APPDIR"), std::env::var_os("PATH")) {
            let paths = std::env::split_paths(&path)
                .filter(|p| !p.starts_with(&mount))
                .collect::<Vec<_>>();
            if let Ok(path) = std::env::join_paths(paths) {
                c.env("PATH", path);
            }
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000);
    }
    c
}

fn launch(plan: &Plan, health: bool) -> Result<std::process::Child, String> {
    let exe = installed_executable(plan);
    let mut c = command(&exe);
    c.args(&plan.launch_args).current_dir(
        plan.destination
            .parent()
            .ok_or("Missing installation directory")?,
    );
    c.env("PEPOMOTE_UPDATE_RESULT", plan.work.join("result"));
    if health {
        c.env("PEPOMOTE_UPDATE_HEALTH", plan.work.join("health"));
    }
    // Fallback launches belong to this group too, so rollback cannot leave a
    // surviving new process accessing the restored installation.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        c.process_group(0);
    }
    c.spawn().map_err(|e| e.to_string())
}

fn stop_child(child: &mut std::process::Child) -> Result<(), String> {
    #[cfg(unix)]
    {
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
    }
    let _ = child.kill();
    child.wait().map_err(|e| e.to_string())?;
    Ok(())
}

fn transaction(plan: &Plan, health_timeout: Duration) -> Result<(), String> {
    if file_hash(&installed_executable(plan))? != plan.original_sha256 {
        return Err("Installation changed while downloading; check for updates again".into());
    }
    // Check again immediately before touching the installation, not only at download time.
    let package = if plan.bundle {
        plan.work.join("package")
    } else {
        plan.staged.clone()
    };
    verify_stream(
        fs::File::open(package).map_err(|e| e.to_string())?,
        std::io::sink(),
        &plan.asset,
        &AtomicBool::new(false),
        |_| {},
    )?;
    if let Err(error) = replace(plan) {
        rollback(plan)?;
        return Err(error);
    }
    let mut child = match launch(plan, true) {
        Ok(c) => c,
        Err(e) => {
            rollback(plan)?;
            return Err(e);
        }
    };
    let began = Instant::now();
    while began.elapsed() < health_timeout {
        if let Ok(health) = fs::read_to_string(plan.work.join("health")) {
            if health.trim() == format!("{}:{}", plan.version, child.id())
                && child.try_wait().map_err(|e| e.to_string())?.is_none()
            {
                durable_write(&plan.work.join("result"), b"installed")?;
                return Ok(());
            }
            // Linux's existing GL fallback re-execs; validate its reported PID
            // belongs to this update's process group before accepting it.
            #[cfg(unix)]
            if let Some((version, pid)) = health.trim().split_once(':') {
                if let Ok(pid) = pid.parse::<i32>() {
                    if version == plan.version
                        && pid > 1
                        && unsafe { libc::getpgid(pid) } == child.id() as i32
                    {
                        durable_write(&plan.work.join("result"), b"installed")?;
                        return Ok(());
                    }
                }
            }
        }
        // Allow a GL fallback child to report health even when the launcher exits.
        #[cfg(windows)]
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    stop_child(&mut child)?;
    rollback(plan)?;
    Err("New version did not confirm startup; the previous version was restored".into())
}

pub fn confirm_health() {
    static STARTED: AtomicBool = AtomicBool::new(false);
    let Some(path) = std::env::var_os("PEPOMOTE_UPDATE_HEALTH") else {
        return;
    };
    if STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    // First UI frame + three seconds of process liveness. The timer belongs
    // to the process: minimizing a healthy window must not cause rollback.
    let result = std::thread::Builder::new()
        .name("pmp-update-health".into())
        .spawn(move || {
            std::thread::sleep(Duration::from_secs(3));
            let _ = durable_write(
                Path::new(&path),
                format!("{}:{}", super::Version::current(), std::process::id()).as_bytes(),
            );
        });
    if result.is_err() {
        STARTED.store(false, Ordering::Relaxed);
    }
}

pub fn prepare(
    manifest: &Manifest,
    cancel: &AtomicBool,
    progress: impl FnMut(u64),
) -> Result<Plan, String> {
    let runtime = Runtime::current()?;
    let target = select_target(&runtime)?;
    let asset = manifest
        .assets
        .get(target.key)
        .ok_or("No package for this installation")?
        .clone();
    reject_links(&target.destination)?;
    let destination = target
        .destination
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let parent = destination
        .parent()
        .ok_or("Missing installation directory")?;
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let work = parent.join(format!(".pepomote-update-{}-{unique}", std::process::id()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&work)
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    fs::create_dir(&work).map_err(|e| e.to_string())?;
    let package = work.join("package");
    let result = (|| {
        let response = super::get_trusted(&asset.url, Duration::from_secs(30))?;
        if response
            .header("Content-Length")
            .is_some_and(|n| n.parse::<u64>().ok() != Some(asset.size))
        {
            return Err("Package size changed".into());
        }
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&package)
            .map_err(|e| e.to_string())?;
        verify_stream(
            response.into_reader(),
            &mut output,
            &asset,
            cancel,
            progress,
        )?;
        output.sync_all().map_err(|e| e.to_string())?;
        drop(output);
        let staged = if target.bundle {
            unpack_bundle(&package, &work, cancel)?
        } else {
            executable(&package)?;
            package.clone()
        };
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let helper = work.join(if cfg!(windows) {
            "helper.exe"
        } else if runtime.appimage.is_some() {
            "helper.AppImage"
        } else {
            "helper"
        });
        fs::copy(
            runtime.appimage.as_ref().unwrap_or(&runtime.executable),
            &helper,
        )
        .map_err(|e| e.to_string())?;
        executable(&helper)?;
        let original_sha256 = file_hash(&if target.bundle {
            destination.join("Contents/MacOS/PepoMote")
        } else {
            destination.clone()
        })?;
        let plan = Plan {
            work: work.clone(),
            destination,
            staged,
            bundle: target.bundle,
            asset,
            version: manifest.version.clone(),
            parent_pid: std::process::id(),
            launch_args: Vec::new(),
            original_sha256,
        };
        durable_write(
            &work.join("release.json"),
            &serde_json::to_vec(manifest).map_err(|e| e.to_string())?,
        )?;
        durable_write(
            &work.join("plan.json"),
            &serde_json::to_vec(&plan).map_err(|e| e.to_string())?,
        )?;
        Ok(plan)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&work);
    }
    result
}

fn unpack_bundle(package: &Path, work: &Path, cancel: &AtomicBool) -> Result<PathBuf, String> {
    let mut zip = zip::ZipArchive::new(fs::File::open(package).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if zip.len() > 20_000 {
        return Err("Too many package entries".into());
    }
    let root = work.join("unpacked");
    fs::create_dir(&root).map_err(|e| e.to_string())?;
    let mut total = 0u64;
    for i in 0..zip.len() {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        let relative = f.enclosed_name().ok_or("Unsafe ZIP path")?;
        // ditto may include harmless AppleDouble metadata outside the app.
        if relative.starts_with("__MACOSX") {
            continue;
        }
        if !relative.starts_with("PepoMote.app")
            || f.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
        {
            return Err("Unexpected bundle entry / symbolic link".into());
        }
        total = total.checked_add(f.size()).ok_or("Bundle too large")?;
        if total > super::manifest::MAX_PACKAGE {
            return Err("Bundle too large".into());
        }
        let path = root.join(&relative);
        if f.is_dir() {
            fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            continue;
        }
        fs::create_dir_all(path.parent().ok_or("Invalid bundle path")?)
            .map_err(|e| e.to_string())?;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        let size = f.size();
        let mut limited = (&mut f).take(size + 1);
        if std::io::copy(&mut limited, &mut file).map_err(|e| e.to_string())? != size {
            return Err("Bundle entry size mismatch".into());
        }
        file.sync_all().map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                &path,
                fs::Permissions::from_mode(f.unix_mode().unwrap_or(0o644) & 0o777),
            )
            .map_err(|e| e.to_string())?;
        }
    }
    let app = root.join("PepoMote.app");
    if !app.join("Contents/MacOS/PepoMote").is_file() || !app.join("Contents/Info.plist").is_file()
    {
        return Err("Incomplete application bundle".into());
    }
    #[cfg(target_os = "macos")]
    {
        let ok = Command::new("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(&app)
            .status()
            .map_err(|e| e.to_string())?;
        if !ok.success() {
            return Err("Application signature verification failed".into());
        }
    }
    Ok(app)
}

pub fn start_helper(plan: &Plan) -> Result<(), String> {
    validate_plan(plan)?;
    let helper = ["helper.exe", "helper.AppImage", "helper"]
        .iter()
        .map(|s| plan.work.join(s))
        .find(|p| p.is_file())
        .ok_or("Update helper is missing")?;
    let mut child = command(&helper)
        .args([
            std::ffi::OsStr::new("--pepomote-apply-update"),
            plan.work.join("plan.json").as_os_str(),
        ])
        .current_dir(&plan.work)
        .spawn()
        .map_err(|e| e.to_string())?;
    let began = Instant::now();
    while began.elapsed() < Duration::from_secs(15) {
        if plan.work.join("ready").is_file() {
            return Ok(());
        }
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = durable_write(&plan.work.join("abort"), b"abort");
    let _ = child.kill();
    Err("Update helper could not start; installation is unchanged".into())
}

pub fn commit(plan: &Plan) -> Result<(), String> {
    let helper_pid = fs::read_to_string(plan.work.join("ready"))
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .filter(|p| *p > 1)
        .ok_or("Update helper is unavailable; please retry")?;
    let nonce = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    );
    durable_write(&plan.work.join("commit.tmp"), nonce.as_bytes())?;
    fs::rename(plan.work.join("commit.tmp"), plan.work.join("commit"))
        .map_err(|e| e.to_string())?;
    let expected = format!("{helper_pid}:{nonce}");
    let began = Instant::now();
    while began.elapsed() < Duration::from_secs(3) {
        if !parent_alive(helper_pid) {
            break;
        }
        if fs::read_to_string(plan.work.join("committed"))
            .ok()
            .as_ref()
            == Some(&expected)
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = durable_write(&plan.work.join("abort"), b"abort");
    Err("Update helper did not confirm restart; PepoMote remains open. Please retry".into())
}

fn validate_plan(plan: &Plan) -> Result<(), String> {
    reject_links(&plan.work)?;
    reject_links(&plan.destination)?;
    reject_links(&plan.staged)?;
    if plan.work.parent() != plan.destination.parent()
        || !plan
            .work
            .file_name()
            .is_some_and(|s| s.to_string_lossy().starts_with(".pepomote-update-"))
        || !plan.staged.starts_with(&plan.work)
        || plan.parent_pid < 2
        || !plan.launch_args.is_empty()
        || system_path(&plan.destination)
        || super::Version::parse(&plan.version).map(|v| v.to_string()) != Some(plan.version.clone())
    {
        return Err("Invalid update plan".into());
    }
    Ok(())
}

fn parent_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        unsafe {
            libc::kill(pid as i32, 0) == 0
                || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
        }
    }
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut std::ffi::c_void;
            fn WaitForSingleObject(handle: *mut std::ffi::c_void, ms: u32) -> u32;
            fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
        }
        unsafe {
            let h = OpenProcess(0x00100000, 0, pid);
            if h.is_null() {
                return std::io::Error::last_os_error().raw_os_error() != Some(87);
            }
            let running = WaitForSingleObject(h, 0) != 0;
            CloseHandle(h);
            running
        }
    }
}

/// Called before logging, network threads, tray creation or the singleton lock.
pub fn run_from_args() -> bool {
    let args = std::env::args_os().collect::<Vec<_>>();
    if args.get(1).is_none_or(|s| s != "--pepomote-apply-update") {
        return false;
    }
    let result = (|| -> Result<(), String> {
        if args.len() != 3 {
            return Err("Missing update plan".into());
        }
        let path = PathBuf::from(&args[2]);
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        if bytes.len() > 64 * 1024 {
            return Err("Update plan too large".into());
        }
        let plan: Plan = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        validate_plan(&plan)?;
        if path != plan.work.join("plan.json") {
            return Err("Invalid plan path".into());
        }
        durable_write(
            &plan.work.join("ready"),
            std::process::id().to_string().as_bytes(),
        )?;
        let began = Instant::now();
        while !plan.work.join("commit").is_file() {
            if plan.work.join("abort").exists() || began.elapsed() > Duration::from_secs(90) {
                return Err("Update handoff cancelled".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let nonce = fs::read_to_string(plan.work.join("commit")).map_err(|e| e.to_string())?;
        if nonce.is_empty() || nonce.len() > 128 {
            return Err("Invalid commit marker".into());
        }
        durable_write(
            &plan.work.join("committed"),
            format!("{}:{nonce}", std::process::id()).as_bytes(),
        )?;
        let committed = Instant::now();
        while parent_alive(plan.parent_pid) {
            if plan.work.join("abort").exists() || committed.elapsed() > Duration::from_secs(30) {
                return Err("Parent did not exit; installation unchanged".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if plan.work.join("abort").exists() {
            return Err("Update handoff cancelled".into());
        }
        match transaction(&plan, Duration::from_secs(60)) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = durable_write(&plan.work.join("error.txt"), e.as_bytes());
                let restored = file_hash(&installed_executable(&plan)).ok().as_ref()
                    == Some(&plan.original_sha256);
                if !restored {
                    let _ = durable_write(&plan.work.join("result"), b"recovery-required");
                } else {
                    let _ = durable_write(&plan.work.join("result"), b"rolled-back");
                    let _ = launch(&plan, false);
                }
                Err(e)
            }
        }
    })();
    std::process::exit(if result.is_ok() { 0 } else { 1 });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn runtime(os: &str, arch: &str, mobile: bool, musl: bool) -> Runtime {
        Runtime {
            os: os.into(),
            arch: arch.into(),
            mobile,
            musl,
            executable: "/home/test/PepoMote".into(),
            appimage: None,
            managed: false,
        }
    }
    #[test]
    fn exact_artifact_matches_platform_architecture_and_distribution() {
        for (os, arch, mobile, musl, expected) in [
            ("windows", "x86_64", false, false, "windows-x86_64"),
            ("linux", "x86_64", false, false, "linux-x86_64"),
            ("linux", "aarch64", true, false, "mobile-linux-aarch64"),
            ("linux", "aarch64", true, true, "mobile-linux-aarch64-musl"),
        ] {
            assert_eq!(
                select_target(&runtime(os, arch, mobile, musl)).unwrap().key,
                expected
            );
        }
        let mut r = runtime("linux", "x86_64", false, false);
        r.appimage = Some("/home/test/PepoMote.AppImage".into());
        let t = select_target(&r).unwrap();
        assert_eq!(t.key, "linux-x86_64-appimage");
        assert_eq!(t.destination, r.appimage.unwrap());
        assert!(select_target(&runtime("windows", "aarch64", false, false)).is_err());
        let mut r = runtime("linux", "aarch64", true, true);
        r.managed = true;
        assert!(select_target(&r).is_err());
        let mut r = runtime("macos", "aarch64", false, false);
        r.executable = "/Applications/PepoMote.app/Contents/MacOS/PepoMote".into();
        let t = select_target(&r).unwrap();
        assert!(t.bundle);
        assert_eq!(t.destination, PathBuf::from("/Applications/PepoMote.app"));
    }
    #[cfg(windows)]
    #[test]
    fn managed_windows_directory_is_detected_with_canonical_prefix() {
        let Some(program_files) = std::env::var_os("ProgramFiles") else {
            return;
        };
        let path = PathBuf::from(program_files).join("PepoMote/PepoMote.exe");
        assert!(system_path(&path));
        assert!(system_path(&PathBuf::from(format!(
            "\\\\?\\{}",
            path.display()
        ))));
    }
    #[test]
    fn verified_stream_rejects_corruption_short_extra_and_cancelled_bytes() {
        let a = Asset {
            name: "PepoMote.exe".into(),
            url: String::new(),
            size: 3,
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into(),
        };
        let cancel = AtomicBool::new(false);
        let mut out = Vec::new();
        let mut seen = 0;
        verify_stream(&b"abc"[..], &mut out, &a, &cancel, |n| seen = n).unwrap();
        assert_eq!(out, b"abc");
        assert_eq!(seen, 3);
        for bytes in [b"ab".as_slice(), b"abd", b"abcd"] {
            assert!(verify_stream(bytes, Vec::new(), &a, &cancel, |_| {}).is_err());
        }
        cancel.store(true, Ordering::Relaxed);
        assert!(verify_stream(&b"abc"[..], Vec::new(), &a, &cancel, |_| {}).is_err());
    }

    fn scratch() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "pepomote-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&p).unwrap();
        // macOS's temporary directory may pass through /var -> /private/var.
        // The real installer canonicalizes its destination before staging.
        p.canonicalize().unwrap()
    }
    fn test_plan(root: &Path, content: &[u8]) -> Plan {
        let work = root.join(".pepomote-update-test");
        fs::create_dir(&work).unwrap();
        let destination = root.join(if cfg!(windows) {
            "PepoMote.exe"
        } else {
            "PepoMote"
        });
        let staged = work.join("package");
        fs::write(&staged, content).unwrap();
        Plan {
            work,
            destination,
            staged,
            bundle: false,
            asset: Asset {
                name: "fixture".into(),
                url: String::new(),
                size: content.len() as u64,
                sha256: format!("{:x}", Sha256::digest(content)),
            },
            version: super::super::Version::current().to_string(),
            parent_pid: std::process::id(),
            launch_args: Vec::new(),
            original_sha256: format!("{:x}", Sha256::digest(b"old")),
        }
    }
    #[test]
    fn replacement_retains_old_bytes_and_rollback_restores_them() {
        let root = scratch();
        let plan = test_plan(&root, b"new");
        fs::write(&plan.destination, b"old").unwrap();
        replace(&plan).unwrap();
        assert_eq!(fs::read(&plan.destination).unwrap(), b"new");
        assert_eq!(fs::read(plan.work.join("previous")).unwrap(), b"old");
        rollback(&plan).unwrap();
        assert_eq!(fs::read(&plan.destination).unwrap(), b"old");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn missing_staging_never_removes_current_installation() {
        let root = scratch();
        let plan = test_plan(&root, b"new");
        fs::write(&plan.destination, b"old").unwrap();
        fs::remove_file(&plan.staged).unwrap();
        assert!(replace(&plan).is_err());
        assert_eq!(fs::read(&plan.destination).unwrap(), b"old");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn missing_or_partial_backup_never_overwrites_the_only_good_copy() {
        let root = scratch();
        let plan = test_plan(&root, b"new");
        fs::write(&plan.destination, b"old").unwrap();
        fs::write(backup(&plan), b"partial-backup").unwrap();
        rollback(&plan).unwrap();
        assert_eq!(fs::read(&plan.destination).unwrap(), b"old");
        fs::write(&plan.destination, b"new").unwrap();
        assert!(rollback(&plan).is_err());
        assert_eq!(fs::read(&plan.destination).unwrap(), b"new");
        fs::remove_file(backup(&plan)).unwrap();
        assert!(rollback(&plan).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn corrupted_staging_leaves_current_installation_untouched() {
        let root = scratch();
        let plan = test_plan(&root, b"new");
        fs::write(&plan.destination, b"old").unwrap();
        fs::write(&plan.staged, b"bad").unwrap();
        assert!(transaction(&plan, Duration::from_secs(1)).is_err());
        assert_eq!(fs::read(&plan.destination).unwrap(), b"old");
        assert!(!backup(&plan).exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn stale_ready_without_fresh_live_ack_cannot_authorize_shutdown() {
        let root = scratch();
        let plan = test_plan(&root, b"new");
        fs::write(&plan.destination, b"old").unwrap();
        fs::write(plan.work.join("ready"), std::process::id().to_string()).unwrap();
        fs::write(plan.work.join("committed"), "stale-acknowledgment").unwrap();
        assert!(commit(&plan).is_err());
        assert_eq!(fs::read(&plan.destination).unwrap(), b"old");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn fresh_ack_from_a_live_helper_allows_handoff() {
        let root = scratch();
        let plan = test_plan(&root, b"new");
        fs::write(&plan.destination, b"old").unwrap();
        fs::write(plan.work.join("ready"), std::process::id().to_string()).unwrap();
        let work = plan.work.clone();
        let helper = std::thread::spawn(move || {
            let began = Instant::now();
            while !work.join("commit").exists() && began.elapsed() < Duration::from_secs(2) {
                std::thread::sleep(Duration::from_millis(10));
            }
            let nonce = fs::read_to_string(work.join("commit")).unwrap();
            fs::write(
                work.join("committed"),
                format!("{}:{nonce}", std::process::id()),
            )
            .unwrap();
        });
        commit(&plan).unwrap();
        helper.join().unwrap();
        // Commit grants permission; only the separate helper may replace files.
        assert_eq!(fs::read(&plan.destination).unwrap(), b"old");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn archive_traversal_duplicates_and_incomplete_bundles_are_rejected() {
        for entries in [
            vec!["../outside"],
            vec![
                "PepoMote.app/Contents/Info.plist",
                "PepoMote.app/Contents/Info.plist",
            ],
            vec!["PepoMote.app/Contents/Info.plist"],
        ] {
            let root = scratch();
            let package = root.join("archive.zip");
            let mut writer = zip::ZipWriter::new(fs::File::create(&package).unwrap());
            let mut duplicate_rejected = false;
            for name in entries {
                if writer
                    .start_file(name, zip::write::SimpleFileOptions::default())
                    .is_err()
                {
                    duplicate_rejected = true;
                    break;
                }
                writer.write_all(b"fixture").unwrap();
            }
            writer.finish().unwrap();
            assert!(
                duplicate_rejected
                    || unpack_bundle(&package, &root, &AtomicBool::new(false)).is_err()
            );
            assert!(!root.parent().unwrap().join("outside").exists());
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    #[ignore = "subprocess fixture; invoked only by isolated transaction tests"]
    fn fixture_application() {
        let Some(work) = std::env::var_os("PEPOMOTE_UPDATE_HEALTH") else {
            return;
        };
        if std::env::args().any(|v| v == "--nocapture") {
            fs::write(
                PathBuf::from(work),
                format!(
                    "{}:{}",
                    super::super::Version::current(),
                    std::process::id()
                ),
            )
            .unwrap();
            std::thread::sleep(Duration::from_secs(2));
        }
    }
    #[test]
    fn installed_process_must_confirm_health_or_previous_binary_is_restored() {
        for healthy in [true, false] {
            let root = scratch();
            let exe = fs::read(std::env::current_exe().unwrap()).unwrap();
            let mut plan = test_plan(&root, &exe);
            fs::write(&plan.destination, b"old-binary").unwrap();
            plan.original_sha256 = format!("{:x}", Sha256::digest(b"old-binary"));
            plan.launch_args = vec![
                "--ignored".into(),
                "--exact".into(),
                "update::install::tests::fixture_application".into(),
            ];
            if healthy {
                plan.launch_args.push("--nocapture".into());
            }
            let result = transaction(&plan, Duration::from_secs(4));
            if healthy {
                assert!(result.is_ok(), "{result:?}");
                assert_eq!(fs::read(&plan.destination).unwrap(), exe);
            } else {
                assert!(result.is_err());
                assert_eq!(fs::read(&plan.destination).unwrap(), b"old-binary");
            }
            // The fixture exits by itself; no user's application is started or touched.
            std::thread::sleep(Duration::from_secs(3));
            fs::remove_dir_all(root).unwrap();
        }
    }
}
