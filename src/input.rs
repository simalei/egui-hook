use egui::{Event, Key, Pos2, ViewportId};

use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

use windows::Wdk::System::SystemInformation::NtQuerySystemTime;
use crate::app::{CONTEXT, CONTEXT_HDC};

// https://stackoverflow.com/questions/49288552/how-do-i-read-the-win32-wm-move-lparam-x-y-coordinates-in-c
// This is the reimplementation of Microsoft's macros GET_X_LPARAM and GET_Y_LPARAM
// They take lparam as an argument and return x and y of the mouse pointer accordingly
macro_rules! loword {
    ($lp:ident) => {
        ($lp & 0xFFFF) as i16
    };
}

macro_rules! hiword {
    ($lp:ident) => {
        ($lp >> 16 & 0xFFFF) as i16
    };
}

#[inline]
fn get_pointer_pos(lparam: isize) -> Pos2 {
    Pos2 {
        x: loword!(lparam) as f32,
        y: hiword!(lparam) as f32
    }
}

fn get_key(wparam: usize) -> Option<egui::Key> {
    match wparam {
        0x30..=0x39 => unsafe { Some(std::mem::transmute::<_, Key>(wparam as u8 - 0x1F)) },
        0x41..=0x5A => unsafe { Some(std::mem::transmute::<_, Key>(wparam as u8 - 0x26)) },
        0x70..=0x83 => unsafe { Some(std::mem::transmute::<_, Key>(wparam as u8 - 0x3B)) },
        _ => match VIRTUAL_KEY(wparam as u16) {
            VK_DOWN => Some(Key::ArrowDown),
            VK_LEFT => Some(Key::ArrowLeft),
            VK_RIGHT => Some(Key::ArrowRight),
            VK_UP => Some(Key::ArrowUp),
            VK_ESCAPE => Some(Key::Escape),
            VK_TAB => Some(Key::Tab),
            VK_BACK => Some(Key::Backspace),
            VK_RETURN => Some(Key::Enter),
            VK_SPACE => Some(Key::Space),
            VK_INSERT => Some(Key::Insert),
            VK_DELETE => Some(Key::Delete),
            VK_HOME => Some(Key::Home),
            VK_END => Some(Key::End),
            VK_PRIOR => Some(Key::PageUp),
            VK_NEXT => Some(Key::PageDown),
            _ => None,
        },
    }
}
pub struct InputHandler {
    events: Vec<Event>,
    pub requests_reinitialization: bool
}


impl InputHandler {
    pub fn new() -> Self {
        Self {
            events: vec![],
            requests_reinitialization: false
        }
    }

    pub fn handle_message(&mut self, umsg: u32, wparam: usize, lparam: isize) {
        match umsg {
            WM_MOUSEMOVE => {
                self.events.push(Event::PointerMoved(
                    get_pointer_pos(lparam)
                ))
            }
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
                self.events.push(Event::PointerButton {
                    pos: get_pointer_pos(lparam),
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                });
            }
            WM_LBUTTONUP => {
                self.events.push(Event::PointerButton {
                    pos: get_pointer_pos(lparam),
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                });
            }
            WM_RBUTTONDOWN | WM_RBUTTONDBLCLK => {
                self.events.push(Event::PointerButton {
                    pos: get_pointer_pos(lparam),
                    button: egui::PointerButton::Secondary,
                    pressed: true,
                    modifiers: Default::default(),
                });
            }
            WM_RBUTTONUP => {
                self.events.push(Event::PointerButton {
                    pos: get_pointer_pos(lparam),
                    button: egui::PointerButton::Secondary,
                    pressed: false,
                    modifiers: Default::default(),
                });
            }
            WM_MBUTTONDOWN | WM_MBUTTONDBLCLK => {
                self.events.push(Event::PointerButton {
                    pos: get_pointer_pos(lparam),
                    button: egui::PointerButton::Middle,
                    pressed: true,
                    modifiers: Default::default(),
                });
            }
            WM_MBUTTONUP => {
                self.events.push(Event::PointerButton {
                    pos: get_pointer_pos(lparam),
                    button: egui::PointerButton::Middle,
                    pressed: false,
                    modifiers: Default::default(),
                });
            }
            WM_CHAR => {
                if let Some(ch) = char::from_u32(wparam as _) {
                    if !ch.is_control() {
                        self.events.push(Event::Text(ch.into()));
                    }
                }
            }
            WM_MOUSEWHEEL => {
                let delta = (wparam >> 16) as i16 as f32 * 10. / WHEEL_DELTA as f32;
                self.events.push(Event::Scroll(egui::Vec2::new(0., delta)));
            }
            WM_MOUSEHWHEEL => {
                let delta = (wparam >> 16) as i16 as f32 * 10. / WHEEL_DELTA as f32;
                self.events.push(Event::Scroll(egui::Vec2::new(delta, 0.)));
            }
            _msg @ (WM_KEYDOWN | WM_SYSKEYDOWN) => {
                if let Some(key) = get_key(wparam) {
                    self.events.push(Event::Key {
                        pressed: true,
                        modifiers: Default::default(),
                        key,
                        repeat: lparam & (KF_REPEAT as isize) > 0,
                        physical_key: None,
                    });
                }
            }
            _msg @ (WM_KEYUP | WM_SYSKEYUP) => {
                if let Some(key) = get_key(wparam) {
                    self.events.push(Event::Key {
                        pressed: false,
                        modifiers: Default::default(),
                        key,
                        repeat: lparam & (KF_REPEAT as isize) > 0,
                        physical_key: None,
                    });
                }
            }
            WM_SIZE => unsafe {
                // If the size of the window has changed, re-calculate dimensions
                let context = CONTEXT.get_mut().expect("Failed to obtain context");
                context.dimensions = {
                    let hdc = *CONTEXT_HDC.get().expect("Failed to get hdc");
                    let window = windows::Win32::Graphics::Gdi::WindowFromDC(hdc);
                    let mut dimensions = windows::Win32::Foundation::RECT::default();
                    GetClientRect(window, &mut dimensions)
                        .expect("Failed to acquire window's dimensions");
                    [
                        (dimensions.right - dimensions.left).try_into().unwrap(),
                        (dimensions.bottom - dimensions.top).try_into().unwrap()
                    ]
                }
            }
            _ => {}
        }
    }

    pub fn collect_input(&mut self) -> egui::RawInput {
        egui::RawInput {
            viewport_id: ViewportId::ROOT,
            viewports: std::iter::once((ViewportId::ROOT, Default::default())).collect(),
            screen_rect: Default::default(),
            max_texture_side: Default::default(),
            time: Some(Self::get_system_time()),
            predicted_dt: 1.0 / 60.0,
            modifiers: Default::default(),
            events: std::mem::take(&mut self.events),
            hovered_files: vec![],
            dropped_files: vec![],
            focused: true,
        }
    }
    pub fn get_system_time() -> f64 {
        let mut time = 0;
        unsafe {
            NtQuerySystemTime(&mut time);
        }
        (time as f64) / 10_000_000.
    }
}
