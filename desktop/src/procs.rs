//! Procesos en macOS por libproc (API pública desde 10.5): sin bifurcar `ps`
//! cada dos segundos, sin parsear texto. Lo usa el modo automático para
//! saber si Dolphin o Cemu están abiertos y dónde viven.
#![cfg(target_os = "macos")]

use std::ffi::c_void;
use std::path::{Path, PathBuf};

const PROC_ALL_PIDS: u32 = 1;
const PROC_PIDPATHINFO_MAXSIZE: usize = 4096;

#[link(name = "proc")]
extern "C" {
    fn proc_listpids(kind: u32, typeinfo: u32, buffer: *mut c_void, buffersize: i32) -> i32;
    fn proc_pidpath(pid: i32, buffer: *mut c_void, buffersize: u32) -> i32;
}

/// Todos los procesos visibles con la ruta de su ejecutable.
pub fn list() -> Vec<(u32, PathBuf)> {
    let mut out = Vec::new();
    let bytes = unsafe { proc_listpids(PROC_ALL_PIDS, 0, std::ptr::null_mut(), 0) };
    if bytes <= 0 {
        return out;
    }
    // margen para procesos que nazcan entre las dos llamadas
    let mut pids = vec![0i32; bytes as usize / std::mem::size_of::<i32>() + 64];
    let bytes = unsafe {
        proc_listpids(
            PROC_ALL_PIDS,
            0,
            pids.as_mut_ptr() as *mut c_void,
            (pids.len() * std::mem::size_of::<i32>()) as i32,
        )
    };
    if bytes <= 0 {
        return out;
    }
    pids.truncate(bytes as usize / std::mem::size_of::<i32>());
    let mut buf = vec![0u8; PROC_PIDPATHINFO_MAXSIZE];
    for pid in pids {
        if pid <= 0 {
            continue;
        }
        let n = unsafe { proc_pidpath(pid, buf.as_mut_ptr() as *mut c_void, buf.len() as u32) };
        if n > 0 {
            let path = String::from_utf8_lossy(&buf[..n as usize]).into_owned();
            out.push((pid as u32, PathBuf::from(path)));
        }
    }
    out
}

/// ¿Hay un proceso cuyo ejecutable empiece por `prefix` (minúsculas)? Con la
/// carpeta del ejecutable (`…/Cemu.app/Contents/MacOS`), que es donde un
/// Cemu portable tendría `portable/` y un Dolphin `portable.txt`.
pub fn running_with_prefix(prefix: &str) -> (bool, Option<PathBuf>) {
    for (_, path) in list() {
        let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        if name.starts_with(prefix) {
            return (true, path.parent().map(Path::to_path_buf));
        }
    }
    (false, None)
}

/// `X.app` → `X.app/Contents/MacOS` si ahí hay un ejecutable que `has_exe`
/// reconoce; cualquier otra carpeta, tal cual si la reconoce.
pub fn bundle_exe_dir(dir: &Path, has_exe: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    if dir.extension().is_some_and(|e| e == "app") {
        let inner = dir.join("Contents").join("MacOS");
        return has_exe(&inner).then_some(inner);
    }
    has_exe(dir).then(|| dir.to_path_buf())
}
