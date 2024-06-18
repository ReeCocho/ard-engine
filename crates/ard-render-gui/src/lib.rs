use ard_core::core::Tick;
use ard_ecs::prelude::*;
use ard_input::{InputState, Key, MouseButton};
use ard_render_si::consts::GUI_SCENE_TEXTURE_ID;
use ard_window::prelude::*;
use view::GuiView;

pub mod view;

#[derive(Resource)]
pub struct Gui {
    ctx: egui::Context,
    input: egui::RawInput,
    views: Vec<Box<dyn GuiView + 'static>>,
}

#[derive(Default, SystemState)]
pub struct GuiInputCaptureSystem;

#[derive(Default)]
pub struct GuiRunOutput {
    pub full: egui::FullOutput,
    pub primitives: Vec<egui::ClippedPrimitive>,
    pub pixels_per_point: f32,
}

impl Default for Gui {
    fn default() -> Self {
        use egui::{FontFamily::*, FontId, TextStyle};

        let ctx = egui::Context::default();

        // Install fonts
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "Inter-VariableFont".into(),
            egui::FontData::from_static(include_bytes!("../fonts/Inter-VariableFont.ttf")),
        );
        fonts.families.insert(
            Proportional,
            vec![
                "Inter-VariableFont".into(),
                // NOTE: These come packaged by default with egui
                "NotoEmoji-Regular".into(),
                "emoji-icon-font".into(),
            ],
        );

        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Fill);

        ctx.set_fonts(fonts);

        // Define styling
        ctx.style_mut(|style| {
            style.visuals.dark_mode = true;
            style.visuals.window_rounding = egui::Rounding::from(2.0);
            style.visuals.window_shadow = egui::epaint::Shadow::NONE;
            style.text_styles = [
                (TextStyle::Small, FontId::new(11.0, Proportional)),
                (TextStyle::Body, FontId::new(14.0, Proportional)),
                (TextStyle::Button, FontId::new(14.0, Proportional)),
                (TextStyle::Heading, FontId::new(16.0, Proportional)),
                (TextStyle::Monospace, FontId::new(14.0, Monospace)),
            ]
            .into();
        });

        Self {
            ctx,
            input: Default::default(),
            views: Vec::default(),
        }
    }
}

impl Gui {
    pub const SCENE_TEXTURE: egui::TextureId = egui::TextureId::User(GUI_SCENE_TEXTURE_ID as u64);

    pub fn add_view(&mut self, view: impl GuiView + 'static) {
        self.views.push(Box::new(view));
    }

    pub fn gather_input(&mut self, input: &InputState, window: &Window) {
        // Canvas size hint
        self.input.screen_rect = Some(egui::Rect {
            min: egui::Pos2::ZERO,
            max: egui::Pos2::new(
                window.physical_width() as f32,
                window.physical_height() as f32,
            ),
        });

        // // Don't bother gathering input if the mouse is locked
        // if window.set_cursor_grab() {
        //     return;
        // }

        // Copy-paste and text input
        if !input.input_string().is_empty() {
            let mut final_txt = String::with_capacity(input.input_string().bytes().len());

            // Ignore non-printable characters
            for chr in input.input_string().chars() {
                // Gonna be real, I grabbed this from the egui-winit integration. Idk how it works
                let is_printable_char = {
                    let is_in_private_use_area = ('\u{e000}'..='\u{f8ff}').contains(&chr)
                        || ('\u{f0000}'..='\u{ffffd}').contains(&chr)
                        || ('\u{100000}'..='\u{10fffd}').contains(&chr);

                    !is_in_private_use_area && !chr.is_ascii_control()
                };

                if is_printable_char {
                    final_txt.push(chr);
                }
            }

            self.input.events.push(egui::Event::Text(final_txt));
        }

        // Modifiers
        self.input.modifiers.alt |= input.key(Key::LAlt) || input.key(Key::RAlt);
        self.input.modifiers.ctrl |= input.key(Key::LCtrl) || input.key(Key::RCtrl);
        self.input.modifiers.shift |= input.key(Key::LShift) || input.key(Key::RShift);

        // Mouse buttons
        self.handle_mouse_button(MouseButton::Left, egui::PointerButton::Primary, input);
        self.handle_mouse_button(MouseButton::Right, egui::PointerButton::Secondary, input);
        self.handle_mouse_button(MouseButton::Middle, egui::PointerButton::Middle, input);

        // Mouse movement
        let (del_x, del_y) = input.mouse_delta();
        if del_x != 0.0 || del_y != 0.0 {
            let (x, y) = input.mouse_pos();
            self.input
                .events
                .push(egui::Event::PointerMoved(egui::Pos2::new(
                    x as f32, y as f32,
                )));
        }

        let (del_x, del_y) = input.mouse_scroll();
        if del_x != 0.0 || del_y != 0.0 {
            self.input.events.push(egui::Event::Scroll(egui::Vec2::new(
                del_x as f32,
                del_y as f32,
            )));
        }

        // Keyboard input
        fn keyboard_input(
            ard_key: Key,
            egui_key: egui::Key,
            ard_input: &InputState,
            egui_input: &mut egui::RawInput,
        ) {
            if ard_input.key_down_repeat(ard_key) {
                egui_input.events.push(egui::Event::Key {
                    repeat: true,
                    key: egui_key,
                    physical_key: None,
                    pressed: true,
                    modifiers: egui_input.modifiers,
                });
            }
        }

        keyboard_input(Key::Left, egui::Key::ArrowLeft, input, &mut self.input);
        keyboard_input(Key::Right, egui::Key::ArrowRight, input, &mut self.input);
        keyboard_input(Key::Up, egui::Key::ArrowUp, input, &mut self.input);
        keyboard_input(Key::Down, egui::Key::ArrowDown, input, &mut self.input);

        keyboard_input(Key::Escape, egui::Key::Escape, input, &mut self.input);
        keyboard_input(Key::Tab, egui::Key::Tab, input, &mut self.input);
        keyboard_input(Key::Back, egui::Key::Backspace, input, &mut self.input);
        keyboard_input(Key::Return, egui::Key::Enter, input, &mut self.input);
        keyboard_input(Key::Space, egui::Key::Space, input, &mut self.input);

        keyboard_input(Key::Insert, egui::Key::Insert, input, &mut self.input);
        keyboard_input(Key::Delete, egui::Key::Delete, input, &mut self.input);
        keyboard_input(Key::Home, egui::Key::Home, input, &mut self.input);
        keyboard_input(Key::End, egui::Key::End, input, &mut self.input);
        keyboard_input(Key::PageUp, egui::Key::PageUp, input, &mut self.input);
        keyboard_input(Key::PageDown, egui::Key::PageDown, input, &mut self.input);
    }

    pub fn run(
        &mut self,
        tick: Tick,
        commands: &Commands,
        queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> GuiRunOutput {
        self.input.predicted_dt = tick.0.as_secs_f32();
        let mut full = self.ctx.run(std::mem::take(&mut self.input), |ctx| {
            for view in &mut self.views {
                view.show(tick, ctx, commands, queries, res);
            }
        });
        let primitives = self
            .ctx
            .tessellate(std::mem::take(&mut full.shapes), full.pixels_per_point);

        GuiRunOutput {
            full,
            primitives,
            pixels_per_point: self.ctx.pixels_per_point(),
        }
    }

    fn handle_mouse_button(
        &mut self,
        button: MouseButton,
        egui_button: egui::PointerButton,
        input: &InputState,
    ) {
        if input.mouse_button_down(button) {
            let (x, y) = input.mouse_pos();
            self.input.events.push(egui::Event::PointerButton {
                pos: egui::Pos2::new(x as f32, y as f32),
                button: egui_button,
                pressed: true,
                modifiers: self.input.modifiers,
            })
        }

        if input.mouse_button_up(button) {
            let (x, y) = input.mouse_pos();
            self.input.events.push(egui::Event::PointerButton {
                pos: egui::Pos2::new(x as f32, y as f32),
                button: egui_button,
                pressed: false,
                modifiers: self.input.modifiers,
            })
        }
    }
}

impl GuiInputCaptureSystem {
    fn tick(
        &mut self,
        _: Tick,
        _: Commands,
        _: Queries<()>,
        res: Res<(Write<Gui>, Read<InputState>, Read<Windows>)>,
    ) {
        let mut gui = res.get_mut::<Gui>().unwrap();
        let input = res.get::<InputState>().unwrap();
        let windows = res.get::<Windows>().unwrap();
        let window = windows.get(WindowId::primary()).unwrap();
        gui.gather_input(&input, window);
    }
}

impl From<GuiInputCaptureSystem> for System {
    fn from(state: GuiInputCaptureSystem) -> Self {
        SystemBuilder::new(state)
            .with_handler(GuiInputCaptureSystem::tick)
            .build()
    }
}
