//! Platform layer: owns the winit event loop and window, and translates OS
//! events into `loom_core` input.
//!
//! Headless runs never touch this crate. See `docs/architecture.md`.

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::WindowId;

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
/// window is closed. Redraws continuously.
pub fn run_window(config: WindowConfig, app: &mut impl WindowApp) -> Result<(), PlatformError> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut handler = Handler {
        config,
        app,
        window: None,
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
            WindowEvent::RedrawRequested => {
                let control = self.app.redraw();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
                control
            }
            _ => AppControl::Continue,
        };
        self.apply(event_loop, control);
    }
}
