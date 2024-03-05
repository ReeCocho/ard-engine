use std::time::Duration;

use ard_core::core::Tick;
use ard_ecs::prelude::*;
use ard_input::{InputState, Key, MouseButton};
use ard_window::{
    window::{Window, WindowId},
    windows::Windows,
};
use view::GuiView;

pub mod view;

#[derive(Default, Resource)]
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

impl Gui {
    pub fn add_view(&mut self, view: impl GuiView + 'static) {
        self.views.push(Box::new(view));
    }

    pub fn gather_input(&mut self, input: &InputState, window: &Window, dt: Duration) {
        // Update predicted delta time even if the window is minimized
        self.input.predicted_dt += dt.as_secs_f32();

        // Canvas size hint
        self.input.screen_rect = Some(egui::Rect {
            min: egui::Pos2::ZERO,
            max: egui::Pos2::new(
                window.physical_width() as f32,
                window.physical_height() as f32,
            ),
        });

        // Don't bother gathering input if the mouse is locked
        if window.cursor_locked() {
            return;
        }

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
        commands: &Commands,
        queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> GuiRunOutput {
        let mut full = self.ctx.run(std::mem::take(&mut self.input), |ctx| {
            for view in &mut self.views {
                view.show(ctx, commands, queries, res);
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
        tick: Tick,
        _: Commands,
        _: Queries<()>,
        res: Res<(Write<Gui>, Read<InputState>, Read<Windows>)>,
    ) {
        let mut gui = res.get_mut::<Gui>().unwrap();
        let input = res.get::<InputState>().unwrap();
        let windows = res.get::<Windows>().unwrap();
        let window = windows.get(WindowId::primary()).unwrap();
        gui.gather_input(&input, window, tick.0);
    }
}

impl From<GuiInputCaptureSystem> for System {
    fn from(state: GuiInputCaptureSystem) -> Self {
        SystemBuilder::new(state)
            .with_handler(GuiInputCaptureSystem::tick)
            .build()
    }
}