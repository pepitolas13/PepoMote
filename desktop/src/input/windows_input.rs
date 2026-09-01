use super::{Injector, KeyCode, MouseButton};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
    MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL,
    MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_MEDIA_NEXT_TRACK,
    VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK, VK_RETURN, VK_RIGHT, VK_UP, VK_VOLUME_DOWN,
    VK_VOLUME_MUTE, VK_VOLUME_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
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
            self.send_mouse(
                ax,
                ay,
                0,
                MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
            );
        }
    }

    fn button(&mut self, btn: MouseButton, down: bool) {
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
        };
        self.send_key(vk, down);
    }

    fn wheel(&mut self, delta: i32) {
        self.send_mouse(0, 0, delta, MOUSEEVENTF_WHEEL);
    }
}
