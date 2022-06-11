use ard_ecs::prelude::*;
use ard_graphics_api::prelude::DebugGuiApi;
use imgui::{DrawData, Ui};

use crate::VkBackend;
#[derive(Resource)]
pub struct DebugGui {
    pub(crate) context: Box<imgui::Context>,
    pub(crate) ui: Option<imgui::Ui<'static>>,
}

// Required because the imgui Context contains an internal Rc. Since ECS resources maintain the
// Rust sharing rules, this is fine.
unsafe impl Send for DebugGui {}
unsafe impl Sync for DebugGui {}

impl DebugGui {
    pub(crate) fn new() -> Self {
        let mut context = Box::new(imgui::Context::create());
        let io = context.io_mut();
        io.backend_flags
            .insert(imgui::BackendFlags::RENDERER_HAS_VTX_OFFSET);
        io.display_size = [1.0, 1.0];

        Self { context, ui: None }
    }
}

impl DebugGui {
    // Finish rendering (if it was begun) so that we can draw to the screen.
    pub(crate) fn finish_draw(&mut self) -> Option<&DrawData> {
        let ui = match self.ui.take() {
            Some(ui) => ui,
            None => return None,
        };

        Some(ui.render())
    }
}

impl DebugGuiApi<VkBackend> for DebugGui {
    fn ui(&mut self) -> &mut Ui {
        // Unsafe: Alright. This is hugely gross and ugly and not good. I am aware. The `Ui`
        // imgui object holds onto a reference to the `Context` object internally. This is why
        // there is a lifetime tied to the `Ui` object. The problem is that this means we have
        // to have all the gui code ready to run one we begin the frame. This makes it practically
        // useless for debugging situations where we need to inspect multiple systems. To solve
        // this issue, we erase the lifetime from `Ui` using a transmute (again, I know it's bad).
        // Since the `Context` is boxed, we can guarantee that it has a stable reference and thus
        // that reference will never be invalidated. All user facing references are tied to the
        // object lifetime, so we can't get dangling references.
        unsafe {
            match self.ui.as_mut() {
                Some(ui) => std::mem::transmute(ui),
                None => {
                    self.ui = Some(std::mem::transmute(self.context.frame()));
                    std::mem::transmute(self.ui.as_mut().unwrap())
                }
            }
        }
    }
}
