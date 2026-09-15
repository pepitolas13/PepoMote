//! Activar la ventana de Eden antes de inyectar texto Unicode desde telemetría.
#[cfg(windows)]
pub fn focus_keyboard() -> bool {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetForegroundWindow, GetLastActivePopup, GetWindowThreadProcessId,
        IsWindowVisible,
    };
    unsafe fn is_eden(hwnd: HWND) -> bool {
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let mut buf = [0u16; 2048];
        let mut len = buf.len() as u32;
        let ok =
            QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len)
                .is_ok();
        let _ = CloseHandle(h);
        if !ok {
            return false;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]).to_lowercase();
        matches!(
            path.rsplit(['\\', '/']).next(),
            Some("eden.exe" | "eden-cli.exe")
        )
    }
    unsafe extern "system" fn collect(hwnd: HWND, param: LPARAM) -> BOOL {
        if IsWindowVisible(hwnd).as_bool() && is_eden(hwnd) {
            (*(param.0 as *mut Vec<HWND>)).push(hwnd);
        }
        BOOL(1)
    }
    unsafe {
        if is_eden(GetForegroundWindow()) {
            return true;
        }
        let mut windows = Vec::new();
        let _ = EnumWindows(
            Some(collect),
            LPARAM(&mut windows as *mut Vec<HWND> as isize),
        );
        for hwnd in windows {
            let popup = GetLastActivePopup(hwnd);
            let target = if !popup.0.is_null() && IsWindowVisible(popup).as_bool() && is_eden(popup)
            {
                popup
            } else {
                hwnd
            };
            if crate::input::windows_input::focus_for_text(target) {
                return true;
            }
        }
    }
    false
}

#[cfg(target_os = "macos")]
pub fn focus_keyboard() -> bool {
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
    for (pid, path) in crate::procs::list() {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if !matches!(name.as_str(), "eden" | "eden-cli") {
            continue;
        }
        unsafe {
            if let Some(app) =
                NSRunningApplication::runningApplicationWithProcessIdentifier(pid as i32)
            {
                if app.isActive() {
                    return true;
                }
                if app.activateWithOptions(
                    NSApplicationActivationOptions::NSApplicationActivateIgnoringOtherApps,
                ) {
                    for _ in 0..20 {
                        if app.isActive() {
                            return true;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                }
            }
        }
    }
    false
}

#[cfg(target_os = "linux")]
pub fn focus_keyboard() -> bool {
    // En Wayland no existe una API global para robar el foco. Igual que
    // Cemu, el diálogo de teclado de Eden debe estar en primer plano.
    super::running_exe().0
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn focus_keyboard() -> bool {
    false
}
