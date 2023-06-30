pub mod window;
pub mod windows;

pub mod prelude {
    pub use crate::window::Window;
    pub use crate::window::WindowCommand;
    pub use crate::window::WindowDescriptor;
    pub use crate::window::WindowId;
    pub use crate::window::WindowMode;
    pub use crate::window::WindowResizeConstraints;
    pub use crate::windows::Windows;
    pub use crate::WindowClosed;
    pub use crate::WindowFileDropped;
    pub use crate::WindowPlugin;
    pub use crate::WindowResized;
}

use std::path::PathBuf;

use ard_core::prelude::*;
use ard_ecs::{prelude::*, resource::res::Res, system::commands::Commands};
use prelude::WindowId;

/// Plugin for the window subsystem.
pub struct WindowPlugin {
    pub add_primary_window: Option<window::WindowDescriptor>,
    pub exit_on_close: bool,
}

/// Event that is sent when a window is closed.
#[derive(Debug, Event, Copy, Clone)]
pub struct WindowClosed(pub window::WindowId);

/// Event that is sent when a file is dropped onto a window.
#[derive(Debug, Event, Clone)]
pub struct WindowFileDropped {
    pub window: window::WindowId,
    pub file: PathBuf,
}

/// A window was resized.
#[derive(Debug, Event, Copy, Clone)]
pub struct WindowResized {
    pub id: window::WindowId,
    pub width: u32,
    pub height: u32,
}

impl Default for WindowPlugin {
    fn default() -> Self {
        WindowPlugin {
            add_primary_window: Some(window::WindowDescriptor::default()),
            exit_on_close: true,
        }
    }
}

impl Plugin for WindowPlugin {
    fn build(&mut self, app: &mut AppBuilder) {
        let mut windows = windows::Windows::default();

        if let Some(descriptor) = std::mem::take(&mut self.add_primary_window) {
            windows.create(WindowId::primary(), descriptor);
        }

        if self.exit_on_close {
            app.add_system(ExitOnClose);
        }

        app.add_resource(windows);
    }
}

/// System which signals the engine to stop when the primary window closes.
#[derive(SystemState)]
struct ExitOnClose;

impl ExitOnClose {
    fn window_closed(&mut self, evt: WindowClosed, commands: Commands, _: Queries<()>, _: Res<()>) {
        // If the window was the main window, send a message to end the program
        if evt.0 == WindowId::primary() {
            commands.events.submit(Stop);
        }
    }
}

#[allow(clippy::from_over_into)]
impl Into<System> for ExitOnClose {
    fn into(self) -> System {
        SystemBuilder::new(self)
            .with_handler(ExitOnClose::window_closed)
            .build()
    }
}
