use super::{Injector, KeyCode, MouseButton};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
    MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL,
    VkKeyScanW, KEYEVENTF_UNICODE, MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY, VK_BACK, VK_DOWN,
    VK_ESCAPE, VK_LEFT, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK, VK_RETURN,
    VK_RIGHT, VK_SHIFT, VK_SPACE, VK_UP, VK_VOLUME_DOWN, VK_VOLUME_MUTE, VK_VOLUME_UP,
};
use windows::Win32::Foundation::POINT;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_MENU;
use windows::Win32::System::Threading::AttachThreadInput;
use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetCursorPos, GetForegroundWindow, GetSystemMetrics, GetWindowThreadProcessId,
    SetForegroundWindow, WindowFromPoint, GA_ROOT, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

pub struct WinInjector;

impl WinInjector {
    pub fn new() -> Self {
        Self
    }

    fn send_mouse(&self, dx: i32, dy: i32, data: i32, flags: MOUSE_EVENT_FLAGS) {
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx,
                    dy,
                    mouseData: data as u32,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        unsafe {
            SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        }
    }

    /// Un clic de verdad lleva a primer plano la ventana sobre la que cae.
    /// Con un clic inyectado Windows no siempre lo hace (el derecho de
    /// primer plano es del proceso que «recibió la última entrada», y el
    /// nuestro solo la inyecta): se activa a mano, como haría el ratón.
    /// Primero por las buenas; si el sistema se resiste, enganchando la cola
    /// de entrada del hilo en primer plano; y si tampoco, con la pulsación de
    /// ALT que da el derecho (el truco estándar), soltándola ya activada.
    fn activate_window_under_cursor(&self) {
        unsafe {
            let mut pt = POINT::default();
            if GetCursorPos(&mut pt).is_err() {
                return;
            }
            let hit = WindowFromPoint(pt);
            if hit.0.is_null() {
                return;
            }
            let mut root = GetAncestor(hit, GA_ROOT);
            if root.0.is_null() {
                root = hit;
            }
            let fg = GetForegroundWindow();
            if root == fg {
                return;
            }
            if SetForegroundWindow(root).as_bool() {
                return;
            }
            // Cola de entrada compartida con el hilo que está en primer plano
            let mut fg_thread = 0;
            if !fg.0.is_null() {
                fg_thread = GetWindowThreadProcessId(fg, None);
            }
            let me = GetCurrentThreadId();
            let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, true).as_bool();
            let ok = SetForegroundWindow(root).as_bool();
            if attached {
                let _ = AttachThreadInput(me, fg_thread, false);
            }
            if ok {
                return;
            }
            self.send_key(VK_MENU, true);
            let _ = SetForegroundWindow(root);
            self.send_key(VK_MENU, false);
        }
    }

    fn send_key(&self, vk: VIRTUAL_KEY, down: bool) {
        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if down {
                        Default::default()
                    } else {
                        KEYEVENTF_KEYUP
                    },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        unsafe {
            SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
        }
    }
}

impl Injector for WinInjector {
    fn name(&self) -> &'static str {
        "SendInput"
    }

    fn move_rel(&mut self, dx: i32, dy: i32) {
        self.send_mouse(dx, dy, 0, MOUSEEVENTF_MOVE);
    }

    fn move_abs(&mut self, nx: f32, ny: f32) {
        // nx/ny son 0..1 sobre la PANTALLA PRIMARIA; el escritorio virtual
        // puede empezar en coordenadas negativas con varios monitores.
        unsafe {
            let px = nx as f64 * GetSystemMetrics(SM_CXSCREEN) as f64;
            let py = ny as f64 * GetSystemMetrics(SM_CYSCREEN) as f64;
            let vx = GetSystemMetrics(SM_XVIRTUALSCREEN) as f64;
            let vy = GetSystemMetrics(SM_YVIRTUALSCREEN) as f64;
            let vw = GetSystemMetrics(SM_CXVIRTUALSCREEN) as f64;
            let vh = GetSystemMetrics(SM_CYVIRTUALSCREEN) as f64;
            if vw <= 0.0 || vh <= 0.0 {
                return;
            }
            let ax = (((px - vx) / vw * 65535.0).round() as i32).clamp(0, 65535);
            let ay = (((py - vy) / vh * 65535.0).round() as i32).clamp(0, 65535);
            if std::env::var_os("PEPOMOTE_DEBUG_ABS").is_some() {
                static ONCE: std::sync::Once = std::sync::Once::new();
                ONCE.call_once(|| {
                    let _ = std::fs::write(
                        "C:\\PepoMote\\abs_debug.txt",
                        format!(
                            "nx={nx} ny={ny}\nSM_CX={} SM_CY={}\nvx={vx} vy={vy} vw={vw} vh={vh}\npx={px} py={py} ax={ax} ay={ay}\n",
                            GetSystemMetrics(SM_CXSCREEN),
                            GetSystemMetrics(SM_CYSCREEN)
                        ),
                    );
                });
            }
            self.send_mouse(
                ax,
                ay,
                0,
                MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
            );
        }
    }

    fn button(&mut self, btn: MouseButton, down: bool) {
        if down {
            self.activate_window_under_cursor();
        }
        let flags = match (btn, down) {
            (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
            (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
            (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
            (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
        };
        self.send_mouse(0, 0, 0, flags);
    }

    fn key(&mut self, key: KeyCode, down: bool) {
        let vk = match key {
            KeyCode::ArrowUp => VK_UP,
            KeyCode::ArrowDown => VK_DOWN,
            KeyCode::ArrowLeft => VK_LEFT,
            KeyCode::ArrowRight => VK_RIGHT,
            KeyCode::Enter => VK_RETURN,
            KeyCode::Escape => VK_ESCAPE,
            KeyCode::VolumeUp => VK_VOLUME_UP,
            KeyCode::VolumeDown => VK_VOLUME_DOWN,
            KeyCode::Mute => VK_VOLUME_MUTE,
            KeyCode::PlayPause => VK_MEDIA_PLAY_PAUSE,
            KeyCode::NextTrack => VK_MEDIA_NEXT_TRACK,
            KeyCode::PrevTrack => VK_MEDIA_PREV_TRACK,
            KeyCode::Backspace => VK_BACK,
            KeyCode::Space => VK_SPACE,
            KeyCode::Shift => VK_SHIFT,
            // tecla virtual del carácter en la disposición actual (byte bajo)
            KeyCode::Char(c) => VIRTUAL_KEY((unsafe { VkKeyScanW(c as u16) } & 0xFF) as u16),
        };
        self.send_key(vk, down);
    }

    /// Texto tal cual (cualquier carácter, vía KEYEVENTF_UNICODE) a la
    /// ventana con el foco.
    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            match c {
                '\n' => {
                    self.send_key(VK_RETURN, true);
                    self.send_key(VK_RETURN, false);
                }
                '\r' => {}
                '\u{8}' | '\u{7f}' => {
                    self.send_key(VK_BACK, true);
                    self.send_key(VK_BACK, false);
                }
                _ => {
                    let mut units = [0u16; 2];
                    for u in c.encode_utf16(&mut units) {
                        for up in [false, true] {
                            let input = INPUT {
                                r#type: INPUT_KEYBOARD,
                                Anonymous: INPUT_0 {
                                    ki: KEYBDINPUT {
                                        wVk: VIRTUAL_KEY(0),
                                        wScan: *u,
                                        dwFlags: if up {
                                            KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
                                        } else {
                                            KEYEVENTF_UNICODE
                                        },
                                        time: 0,
                                        dwExtraInfo: 0,
                                    },
                                },
                            };
                            unsafe {
                                SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
                            }
                        }
                    }
                }
            }
        }
    }

    fn wheel(&mut self, delta: i32) {
        self.send_mouse(0, 0, delta, MOUSEEVENTF_WHEEL);
    }

    fn cursor_pos(&mut self) -> Option<(f32, f32)> {
        let mut p = POINT::default();
        unsafe {
            if GetCursorPos(&mut p).is_ok() {
                let w = GetSystemMetrics(SM_CXSCREEN);
                let h = GetSystemMetrics(SM_CYSCREEN);
                if w > 0 && h > 0 {
                    return Some((p.x as f32 / w as f32, p.y as f32 / h as f32));
                }
            }
        }
        None
    }
}
