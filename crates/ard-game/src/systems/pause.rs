use std::time::Duration;

use ard_core::core::{Stop, Tick};
use ard_ecs::prelude::*;
use ard_input::{InputState, Key};
use ard_pal::prelude::MultiSamples;
use ard_render::{MsaaSettings, PresentationSettings};
use ard_render_gui::view::GuiView;
use ard_render_image_effects::smaa::SmaaSettings;
use ard_window::{window::WindowId, windows::Windows};

use crate::{settings::GameSettings, GameRunning, GameStart, GameStop};

#[derive(SystemState)]
pub struct PauseSystem;

#[derive(Default)]
pub enum PauseGui {
    #[default]
    Main,
    Graphics,
}

impl PauseSystem {
    fn tick(
        &mut self,
        _: Tick,
        commands: Commands,
        _: Queries<()>,
        res: Res<(Read<GameRunning>, Read<InputState>)>,
    ) {
        if res.get::<InputState>().unwrap().key_down(Key::Escape) {
            let running = res.get::<GameRunning>().unwrap();
            if running.0 {
                commands.events.submit(GameStop);
            } else {
                commands.events.submit(GameStart);
            }
        }
    }
}

impl GuiView for PauseGui {
    fn show(
        &mut self,
        _tick: Tick,
        ctx: &egui::Context,
        commands: &Commands,
        queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) {
        let mut windows = res.get_mut::<Windows>().unwrap();
        let window = windows.get_mut(WindowId::primary()).unwrap();

        if res.get::<GameRunning>().unwrap().0 {
            window.set_cursor_lock_mode(true);
            window.set_cursor_visibility(false);
            return;
        } else {
            window.set_cursor_lock_mode(false);
            window.set_cursor_visibility(true);
        }

        let screen_rect = ctx.screen_rect();
        egui::Window::new("Paused")
            .collapsible(false)
            .movable(false)
            .pivot(egui::Align2::CENTER_CENTER)
            .min_width(100.0)
            .current_pos(egui::pos2(
                screen_rect.size().x * 0.5,
                screen_rect.size().y * 0.5,
            ))
            .show(ctx, |ui| match self {
                PauseGui::Main => self.show_main(ui, commands, queries, res),
                PauseGui::Graphics => self.show_graphics(ui, commands, queries, res),
            });
    }
}

impl PauseGui {
    fn show_main(
        &mut self,
        ui: &mut egui::Ui,
        commands: &Commands,
        _queries: &Queries<Everything>,
        _res: &Res<Everything>,
    ) {
        ui.vertical_centered_justified(|ui| {
            if ui.button("Graphics Settings").clicked() {
                *self = PauseGui::Graphics;
            }

            if ui.button("Return To Game").clicked() {
                commands.events.submit(GameStart);
            }

            if ui.button("Exit To OS").clicked() {
                commands.events.submit(Stop);
            }
        });
    }

    fn show_graphics(
        &mut self,
        ui: &mut egui::Ui,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) {
        let mut settings = res.get_mut::<GameSettings>().unwrap();

        egui::Grid::new("graphics_settings_grid").show(ui, |ui| {
            ui.label("SMAA Enabled");
            ui.checkbox(&mut settings.smaa, "");
            ui.end_row();

            ui.label("MSAA Sample Count");
            egui::ComboBox::new("msaa_setting", "")
                .selected_text(format!("{:?}", settings.msaa))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut settings.msaa, MultiSamples::Count1, "Disabled");
                    ui.selectable_value(&mut settings.msaa, MultiSamples::Count2, "2x");
                    ui.selectable_value(&mut settings.msaa, MultiSamples::Count4, "4x");
                    ui.selectable_value(&mut settings.msaa, MultiSamples::Count8, "8x");
                });
            ui.end_row();

            ui.label("Target Frame Rate");
            match &mut settings.target_frame_rate {
                Some(value) => {
                    ui.add(egui::DragValue::new(value).range(30..=u32::MAX));
                    let mut checked = true;
                    ui.checkbox(&mut checked, "Disable");
                    if !checked {
                        settings.target_frame_rate = None;
                    }
                }
                None => {
                    let mut checked = false;
                    ui.checkbox(&mut checked, "Enable");
                    if checked {
                        settings.target_frame_rate = Some(60);
                    }
                }
            }
        });

        ui.vertical_centered_justified(|ui| {
            if ui.button("Apply").clicked() {
                res.get_mut::<SmaaSettings>().unwrap().enabled = settings.smaa;
                res.get_mut::<MsaaSettings>().unwrap().samples = settings.msaa;
                res.get_mut::<PresentationSettings>().unwrap().render_time = settings
                    .target_frame_rate
                    .map(|v| Duration::from_secs_f32(1.0 / v.max(30) as f32));
                settings.save();
            }

            if ui.button("Back").clicked() {
                *self = PauseGui::Main;
            }
        });
    }
}

impl From<PauseSystem> for System {
    fn from(value: PauseSystem) -> Self {
        SystemBuilder::new(value)
            .with_handler(PauseSystem::tick)
            .build()
    }
}
