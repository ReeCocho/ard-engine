pub mod window;
pub mod windows;

pub mod prelude {
    pub use crate::window::*;
    pub use crate::windows::*;
    pub use crate::*;
}

use ard_core::prelude::*;
use ard_ecs::prelude::*;
use prelude::WindowId;

/// Plugin for the window subsystem.
pub struct WindowPlugin {
    pub add_primary_window: Option<window::WindowDescriptor>,
    pub exit_on_close: bool,
}

/// Event that is sent when a window is closed.
#[derive(Debug, Event, Copy, Clone)]
pub struct WindowClosed(pub window::WindowId);

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
struct ExitOnClose;

impl SystemState for ExitOnClose {
    type Data = ();
    type Resources = ();
}

impl ExitOnClose {
    fn window_closed(&mut self, ctx: Context<Self>, evt: WindowClosed) {
        // If the window was the main window, send a message to end the program
        if evt.0 == WindowId::primary() {
            ctx.events.submit(Stop);
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
