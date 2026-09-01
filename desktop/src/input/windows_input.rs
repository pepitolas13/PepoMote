use super::{Injector, MouseButton};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEINPUT,
    MOUSE_EVENT_FLAGS,
};

pub struct WinInjector;

impl WinInjector {
    pub fn new() -> Self {
        Self
    }

    fn send(&self, dx: i32, dy: i32, data: i32, flags: MOUSE_EVENT_FLAGS) {
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
}

impl Injector for WinInjector {
    fn move_rel(&mut self, dx: i32, dy: i32) {
        self.send(dx, dy, 0, MOUSEEVENTF_MOVE);
    }

    fn button(&mut self, btn: MouseButton, down: bool) {
        let flags = match (btn, down) {
            (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
            (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
            (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
            (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
        };
        self.send(0, 0, 0, flags);
    }

    fn wheel(&mut self, delta: i32) {
        self.send(0, 0, delta, MOUSEEVENTF_WHEEL);
    }
}
