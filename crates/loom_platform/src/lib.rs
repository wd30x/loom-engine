//! Platform layer: owns the winit event loop and window, and translates OS
//! events into `loom_core` input.
//!
//! Headless runs never touch this crate. See `docs/architecture.md`.

use std::sync::Arc;
use std::time::Instant;

use loom_core::input::Key;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{
    DeviceEvent, DeviceId, ElementState, KeyEvent, MouseButton, StartCause, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, WindowId};

/// Re-exported so callers can hold the window (e.g. to create a surface)
/// without depending on winit themselves.
pub use winit::window::Window;

#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("failed to create the event loop: {0}")]
    EventLoop(#[from] winit::error::EventLoopError),
    #[error("failed to create the window: {0}")]
    CreateWindow(#[from] winit::error::OsError),
}

/// What the app wants after handling an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppControl {
    Continue,
    Exit,
}

/// Callbacks from the window event loop. Returning [`AppControl::Exit`] ends
/// the loop; [`run_window`] then returns `Ok(())`. Apps that fail keep their
/// own error and return `Exit`.
pub trait WindowApp {
    /// The window was created. Called exactly once, before any other callback.
    fn window_created(&mut self, window: Arc<Window>) -> AppControl;
    /// The window's inner size changed, in physical pixels. Either side may be
    /// zero while minimized.
    fn resized(&mut self, width: u32, height: u32) -> AppControl;
    /// Time to run and draw one frame.
    fn redraw(&mut self) -> AppControl;
    /// The user asked to close the window. Defaults to exiting.
    fn close_requested(&mut self) -> AppControl {
        AppControl::Exit
    }
    /// Escape was pressed. Defaults to exiting.
    fn escape_pressed(&mut self) -> AppControl {
        AppControl::Exit
    }
    /// A bindable key went down or up. OS key repeats are not reported.
    fn key(&mut self, _key: Key, _down: bool) -> AppControl {
        AppControl::Continue
    }
    /// The window lost keyboard focus; key-up events may be missed from now.
    fn focus_lost(&mut self) -> AppControl {
        AppControl::Continue
    }
    /// The window gained keyboard focus.
    fn focus_gained(&mut self) -> AppControl {
        AppControl::Continue
    }
    /// Raw mouse motion (not affected by pointer acceleration or the screen
    /// edges); right and down are positive. Only reported while the window
    /// has focus.
    fn mouse_motion(&mut self, _dx: f64, _dy: f64) -> AppControl {
        AppControl::Continue
    }
    /// When the next frame is due, asked after every redraw. `None` (the
    /// default) redraws again right away, paced by the present's vsync.
    /// `Some(t)` sleeps until `t` first; use it when nothing is presented
    /// (e.g. minimized) and the loop would otherwise spin.
    fn wait_until(&self) -> Option<Instant> {
        None
    }
}

/// Window creation parameters.
#[derive(Debug, Clone)]
pub struct WindowConfig {
    pub title: String,
    /// Initial inner size in physical pixels.
    pub width: u32,
    pub height: u32,
}

/// Opens a window and runs the event loop until `app` asks to exit or the
/// window is closed. Redraws continuously, or at the times the app asks for
/// through [`WindowApp::wait_until`].
pub fn run_window(config: WindowConfig, app: &mut impl WindowApp) -> Result<(), PlatformError> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut handler = Handler {
        config,
        app,
        window: None,
        focused: false,
        error: None,
    };
    event_loop.run_app(&mut handler)?;
    match handler.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

struct Handler<'a, A> {
    config: WindowConfig,
    app: &'a mut A,
    window: Option<Arc<Window>>,
    /// The window has keyboard focus; mouse motion is only forwarded then.
    focused: bool,
    error: Option<PlatformError>,
}

impl<A: WindowApp> Handler<'_, A> {
    fn apply(&self, event_loop: &ActiveEventLoop, control: AppControl) {
        if control == AppControl::Exit {
            event_loop.exit();
        }
    }
}

impl<A: WindowApp> ApplicationHandler for Handler<'_, A> {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        // The wait requested after the last redraw is over: redraw now.
        if let StartCause::ResumeTimeReached { .. } = cause {
            event_loop.set_control_flow(ControlFlow::Poll);
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title(self.config.title.clone())
            .with_inner_size(PhysicalSize::new(self.config.width, self.config.height));
        let window = match event_loop.create_window(attributes) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                self.error = Some(e.into());
                event_loop.exit();
                return;
            }
        };
        self.window = Some(window.clone());
        let control = self.app.window_created(window.clone());
        self.apply(event_loop, control);
        window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let control = match event {
            WindowEvent::CloseRequested => self.app.close_requested(),
            WindowEvent::Resized(size) => self.app.resized(size.width, size.height),
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::Escape),
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } => self.app.escape_pressed(),
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state,
                        repeat: false,
                        ..
                    },
                ..
            } => match translate_key(code) {
                Some(key) => self.app.key(key, state == ElementState::Pressed),
                None => AppControl::Continue,
            },
            // Captured or not: a click that focuses the window counts too.
            WindowEvent::MouseInput { state, button, .. } => match translate_mouse_button(button) {
                Some(key) => self.app.key(key, state == ElementState::Pressed),
                None => AppControl::Continue,
            },
            WindowEvent::Focused(false) => {
                self.focused = false;
                self.app.focus_lost()
            }
            WindowEvent::Focused(true) => {
                self.focused = true;
                self.app.focus_gained()
            }
            WindowEvent::RedrawRequested => {
                let control = self.app.redraw();
                match self.app.wait_until() {
                    Some(deadline) => event_loop.set_control_flow(ControlFlow::WaitUntil(deadline)),
                    None => {
                        event_loop.set_control_flow(ControlFlow::Poll);
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                }
                control
            }
            _ => AppControl::Continue,
        };
        self.apply(event_loop, control);
    }

    fn device_event(&mut self, event_loop: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if !self.focused {
            return;
        }
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            let control = self.app.mouse_motion(dx, dy);
            self.apply(event_loop, control);
        }
    }
}

/// Hides the pointer and keeps it in the window (`captured`), or gives it
/// back. Capturing first moves the pointer to the middle of the window: on
/// Windows, `Locked` clips the pointer to where it is, which may be outside
/// the window. Tries [`CursorGrabMode::Locked`] first and falls back to
/// [`CursorGrabMode::Confined`]. Failures are logged, not returned: the game
/// still works with a free pointer.
pub fn set_cursor_captured(window: &Window, captured: bool) {
    if captured {
        let center = client_center(window.inner_size());
        if let Err(e) = window.set_cursor_position(center) {
            log::warn!("cannot move the pointer to the middle of the window: {e}");
        }
        if let Err(locked) = window.set_cursor_grab(CursorGrabMode::Locked) {
            if let Err(confined) = window.set_cursor_grab(CursorGrabMode::Confined) {
                log::warn!("cannot capture the mouse (locked: {locked}; confined: {confined})");
            } else {
                log::debug!("mouse confined (locking is not supported: {locked})");
            }
        }
    } else if let Err(e) = window.set_cursor_grab(CursorGrabMode::None) {
        log::warn!("cannot release the mouse: {e}");
    }
    window.set_cursor_visible(!captured);
}

/// The middle of a client area of `size`, rounded down.
fn client_center(size: PhysicalSize<u32>) -> PhysicalPosition<u32> {
    PhysicalPosition::new(size.width / 2, size.height / 2)
}

/// Translates a winit key code into a bindable [`Key`]. `Escape` (reserved
/// for quitting) and keys outside the supported range give `None`.
pub fn translate_key(code: KeyCode) -> Option<Key> {
    Some(match code {
        KeyCode::KeyA => Key::KeyA,
        KeyCode::KeyB => Key::KeyB,
        KeyCode::KeyC => Key::KeyC,
        KeyCode::KeyD => Key::KeyD,
        KeyCode::KeyE => Key::KeyE,
        KeyCode::KeyF => Key::KeyF,
        KeyCode::KeyG => Key::KeyG,
        KeyCode::KeyH => Key::KeyH,
        KeyCode::KeyI => Key::KeyI,
        KeyCode::KeyJ => Key::KeyJ,
        KeyCode::KeyK => Key::KeyK,
        KeyCode::KeyL => Key::KeyL,
        KeyCode::KeyM => Key::KeyM,
        KeyCode::KeyN => Key::KeyN,
        KeyCode::KeyO => Key::KeyO,
        KeyCode::KeyP => Key::KeyP,
        KeyCode::KeyQ => Key::KeyQ,
        KeyCode::KeyR => Key::KeyR,
        KeyCode::KeyS => Key::KeyS,
        KeyCode::KeyT => Key::KeyT,
        KeyCode::KeyU => Key::KeyU,
        KeyCode::KeyV => Key::KeyV,
        KeyCode::KeyW => Key::KeyW,
        KeyCode::KeyX => Key::KeyX,
        KeyCode::KeyY => Key::KeyY,
        KeyCode::KeyZ => Key::KeyZ,
        KeyCode::Digit0 => Key::Digit0,
        KeyCode::Digit1 => Key::Digit1,
        KeyCode::Digit2 => Key::Digit2,
        KeyCode::Digit3 => Key::Digit3,
        KeyCode::Digit4 => Key::Digit4,
        KeyCode::Digit5 => Key::Digit5,
        KeyCode::Digit6 => Key::Digit6,
        KeyCode::Digit7 => Key::Digit7,
        KeyCode::Digit8 => Key::Digit8,
        KeyCode::Digit9 => Key::Digit9,
        KeyCode::ArrowUp => Key::ArrowUp,
        KeyCode::ArrowDown => Key::ArrowDown,
        KeyCode::ArrowLeft => Key::ArrowLeft,
        KeyCode::ArrowRight => Key::ArrowRight,
        KeyCode::Space => Key::Space,
        KeyCode::Enter => Key::Enter,
        KeyCode::Tab => Key::Tab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::ShiftLeft => Key::ShiftLeft,
        KeyCode::ShiftRight => Key::ShiftRight,
        KeyCode::ControlLeft => Key::ControlLeft,
        KeyCode::ControlRight => Key::ControlRight,
        KeyCode::AltLeft => Key::AltLeft,
        KeyCode::AltRight => Key::AltRight,
        KeyCode::Numpad1 => Key::Numpad1,
        KeyCode::Numpad2 => Key::Numpad2,
        KeyCode::Numpad3 => Key::Numpad3,
        KeyCode::Numpad4 => Key::Numpad4,
        KeyCode::Numpad5 => Key::Numpad5,
        KeyCode::Numpad6 => Key::Numpad6,
        KeyCode::Numpad7 => Key::Numpad7,
        KeyCode::Numpad8 => Key::Numpad8,
        KeyCode::Numpad9 => Key::Numpad9,
        KeyCode::Period => Key::Period,
        _ => return None,
    })
}

/// Translates a winit mouse button into a bindable [`Key`]. Buttons other
/// than left, right and middle give `None`.
pub fn translate_mouse_button(button: MouseButton) -> Option<Key> {
    Some(match button {
        MouseButton::Left => Key::MouseLeft,
        MouseButton::Right => Key::MouseRight,
        MouseButton::Middle => Key::MouseMiddle,
        MouseButton::Back | MouseButton::Forward | MouseButton::Other(_) => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn winit_key_codes_translate_to_loom_keys() {
        assert_eq!(translate_key(KeyCode::KeyW), Some(Key::KeyW));
        assert_eq!(translate_key(KeyCode::Digit0), Some(Key::Digit0));
        assert_eq!(translate_key(KeyCode::ArrowUp), Some(Key::ArrowUp));
        assert_eq!(translate_key(KeyCode::ShiftLeft), Some(Key::ShiftLeft));
        assert_eq!(translate_key(KeyCode::Escape), None);
        assert_eq!(translate_key(KeyCode::F5), None);
    }

    #[test]
    fn numpad_digits_and_period_translate_to_their_own_keys() {
        let numpad = [
            (KeyCode::Numpad1, Key::Numpad1),
            (KeyCode::Numpad2, Key::Numpad2),
            (KeyCode::Numpad3, Key::Numpad3),
            (KeyCode::Numpad4, Key::Numpad4),
            (KeyCode::Numpad5, Key::Numpad5),
            (KeyCode::Numpad6, Key::Numpad6),
            (KeyCode::Numpad7, Key::Numpad7),
            (KeyCode::Numpad8, Key::Numpad8),
            (KeyCode::Numpad9, Key::Numpad9),
            (KeyCode::Period, Key::Period),
        ];
        for (code, key) in numpad {
            assert_eq!(translate_key(code), Some(key), "{code:?}");
        }
        // Keys rogue does not bind stay outside the supported range.
        assert_eq!(translate_key(KeyCode::Numpad0), None);
        assert_eq!(translate_key(KeyCode::NumpadDecimal), None);
    }

    #[test]
    fn left_right_and_middle_mouse_buttons_translate_and_others_are_ignored() {
        assert_eq!(
            translate_mouse_button(MouseButton::Left),
            Some(Key::MouseLeft)
        );
        assert_eq!(
            translate_mouse_button(MouseButton::Right),
            Some(Key::MouseRight)
        );
        assert_eq!(
            translate_mouse_button(MouseButton::Middle),
            Some(Key::MouseMiddle)
        );
        assert_eq!(translate_mouse_button(MouseButton::Back), None);
        assert_eq!(translate_mouse_button(MouseButton::Other(7)), None);
    }

    #[test]
    fn client_center_rounds_odd_sizes_down() {
        assert_eq!(
            client_center(PhysicalSize::new(1281, 721)),
            PhysicalPosition::new(640, 360)
        );
        assert_eq!(
            client_center(PhysicalSize::new(1280, 720)),
            PhysicalPosition::new(640, 360)
        );
        assert_eq!(
            client_center(PhysicalSize::new(0, 1)),
            PhysicalPosition::new(0, 0)
        );
    }
}
