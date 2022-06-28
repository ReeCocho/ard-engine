use std::ptr::NonNull;

use ard_ecs::prelude::*;
use ard_graphics_api::prelude::DebugGuiApi;
use ard_input::Key;
use imgui::{DrawData, Ui};

use crate::VkBackend;
#[derive(Resource)]
pub struct DebugGui {
    pub(crate) context: Box<imgui::Context>,
    pub(crate) ui: Option<NonNull<imgui::Ui>>,
}

// Required because the imgui Context contains an internal Rc. Since ECS resources maintain the
// Rust sharing rules, this is fine.
unsafe impl Send for DebugGui {}
unsafe impl Sync for DebugGui {}

impl DebugGui {
    pub(crate) fn new() -> Self {
        let mut context = Box::new(imgui::Context::create());

        // Load font
        let fonts = context.fonts();
        let mut font_config = imgui::FontConfig::default();
        font_config.oversample_h = 2;
        font_config.oversample_v = 2;

        fonts.add_font(&[imgui::FontSource::TtfData {
            data: IMGUI_FONT,
            size_pixels: 18.0,
            config: Some(font_config),
        }]);

        // Configure flags
        let io = context.io_mut();
        io.backend_flags
            .insert(imgui::BackendFlags::RENDERER_HAS_VTX_OFFSET);
        io.config_flags.insert(imgui::ConfigFlags::DOCKING_ENABLE);
        io.display_size = [1.0, 1.0];

        // Key mapping
        io[imgui::Key::Tab] = Key::Tab as _;
        io[imgui::Key::LeftArrow] = Key::Left as _;
        io[imgui::Key::RightArrow] = Key::Right as _;
        io[imgui::Key::UpArrow] = Key::Up as _;
        io[imgui::Key::DownArrow] = Key::Down as _;
        io[imgui::Key::PageUp] = Key::PageUp as _;
        io[imgui::Key::PageDown] = Key::PageDown as _;
        io[imgui::Key::Home] = Key::Home as _;
        io[imgui::Key::End] = Key::End as _;
        io[imgui::Key::Insert] = Key::Insert as _;
        io[imgui::Key::Delete] = Key::Delete as _;
        io[imgui::Key::Backspace] = Key::Back as _;
        io[imgui::Key::Space] = Key::Space as _;
        io[imgui::Key::Enter] = Key::Return as _;
        io[imgui::Key::Escape] = Key::Escape as _;
        io[imgui::Key::KeyPadEnter] = Key::NumEnter as _;
        io[imgui::Key::A] = Key::A as _;
        io[imgui::Key::C] = Key::C as _;
        io[imgui::Key::V] = Key::V as _;
        io[imgui::Key::X] = Key::X as _;
        io[imgui::Key::Y] = Key::Y as _;
        io[imgui::Key::Z] = Key::Z as _;

        // Skin for the editor
        let style = context.style_mut();
        style.frame_padding = [4.0, 2.0];
        style.item_spacing = [10.0, 3.0];
        style.window_padding = [4.0, 4.0];
        style.window_rounding = 2.0;
        style.window_border_size = 1.0;
        style.frame_border_size = 0.0;
        style.frame_rounding = 2.0;
        style.scrollbar_rounding = 2.0;
        style.child_rounding = 2.0;
        style.popup_rounding = 4.0;
        style.grab_rounding = 2.0;
        style.tab_rounding = 2.0;
        style.scrollbar_size = 16.0;
        style.indent_spacing = 18.0;

        style[imgui::StyleColor::WindowBg] = [0.14, 0.14, 0.14, 1.0];
        style[imgui::StyleColor::Border] = [0.14, 0.14, 0.14, 1.0];
        style[imgui::StyleColor::BorderShadow] = [0.1, 0.1, 0.1, 0.35];
        style[imgui::StyleColor::FrameBg] = [0.18, 0.18, 0.18, 1.0];
        style[imgui::StyleColor::FrameBgHovered] = [0.15, 0.15, 0.15, 0.78];
        style[imgui::StyleColor::FrameBgActive] = [0.15, 0.15, 0.15, 0.67];
        style[imgui::StyleColor::TitleBg] = [0.1, 0.1, 0.1, 1.0];
        style[imgui::StyleColor::TitleBgActive] = [0.13, 0.13, 0.13, 1.0];
        style[imgui::StyleColor::MenuBarBg] = [0.14, 0.14, 0.14, 1.0];
        style[imgui::StyleColor::CheckMark] = [0.69, 0.69, 0.69, 1.0];
        style[imgui::StyleColor::Header] = [0.2, 0.2, 0.2, 1.0];
        style[imgui::StyleColor::HeaderHovered] = [0.27, 0.27, 0.27, 1.0];
        style[imgui::StyleColor::HeaderActive] = [0.25, 0.25, 0.25, 1.0];
        style[imgui::StyleColor::Separator] = [0.2, 0.2, 0.2, 1.0];
        style[imgui::StyleColor::SeparatorHovered] = [0.3, 0.3, 0.3, 1.0];
        style[imgui::StyleColor::SeparatorActive] = [0.4, 0.4, 0.4, 1.0];
        style[imgui::StyleColor::Tab] = [0.1, 0.1, 0.1, 1.0];
        style[imgui::StyleColor::TabHovered] = [0.27, 0.27, 0.27, 0.8];
        style[imgui::StyleColor::TabActive] = [0.26, 0.26, 0.26, 1.0];
        style[imgui::StyleColor::TabUnfocused] = [0.14, 0.14, 0.14, 1.0];
        style[imgui::StyleColor::TabUnfocusedActive] = [0.2, 0.2, 0.2, 1.0];
        style[imgui::StyleColor::ChildBg] = [0.2, 0.2, 0.2, 1.0];
        style[imgui::StyleColor::PopupBg] = [0.2, 0.2, 0.2, 1.0];
        style[imgui::StyleColor::TitleBg] = [0.13, 0.13, 0.13, 1.0];
        style[imgui::StyleColor::TitleBgCollapsed] = [0.15, 0.15, 0.15, 1.0];
        style[imgui::StyleColor::ScrollbarBg] = [0.15, 0.15, 0.15, 1.0];
        style[imgui::StyleColor::ModalWindowDimBg] = [0.0, 0.0, 0.0, 0.35];

        Self { context, ui: None }
    }
}

impl DebugGui {
    // Finish rendering (if it was begun) so that we can draw to the screen.
    #[inline]
    pub(crate) fn finish_draw(&mut self) -> Option<&DrawData> {
        if self.ui.take().is_none() {
            return None;
        }

        Some(self.context.render())
    }
}

impl DebugGuiApi<VkBackend> for DebugGui {
    #[inline]
    fn ui(&mut self) -> &Ui {
        unsafe {
            match self.ui.clone() {
                Some(ui) => ui.as_ref(),
                None => {
                    self.ui = Some(NonNull::new_unchecked(
                        self.context.new_frame() as *mut imgui::Ui
                    ));
                    self.ui.clone().unwrap().as_ref()
                }
            }
        }
    }

    /// Used to intitialize a dock space.
    ///
    /// # Note
    /// This is only temporary until the imgui-rs docking API is stable.
    #[inline]
    fn begin_dock(&mut self) {
        // Create ui if it doesn't exist yet
        self.ui();

        // Create the dock space
        unsafe {
            imgui::sys::igDockSpaceOverViewport(
                imgui::sys::igGetMainViewport(),
                imgui::sys::ImGuiDockNodeFlags_None as i32,
                std::ptr::null(),
            );
        }
    }

    #[inline]
    fn font_atlas() -> imgui::TextureId {
        imgui::TextureId::new(u32::MAX as usize)
    }

    #[inline]
    fn scene_view() -> imgui::TextureId {
        imgui::TextureId::new(u32::MAX as usize - 1)
    }
}

const IMGUI_FONT: &[u8] = include_bytes!("./segoeui.ttf");
