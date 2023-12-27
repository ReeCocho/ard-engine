use std::time::Instant;

use ard_core::prelude::Tick;
use ard_engine::{
    ecs::prelude::*,
    math::{EulerRot, Mat4, Vec3, Vec4},
};
use ard_input::{InputState, Key};
use ard_math::Vec4Swizzles;
use ard_render2::camera::{Camera, CameraDescriptor};
use ard_render_objects::Model;
use ard_window::{window::WindowId, windows::Windows};

pub const CUBE_INDICES: &'static [u32] = &[
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
    26, 27, 28, 29, 30, 31, 32, 33, 34, 35,
];

pub const CUBE_VERTICES: &'static [Vec4] = &[
    Vec4::new(-0.5, -0.5, -0.5, 1.0),
    Vec4::new(0.5, -0.5, -0.5, 1.0),
    Vec4::new(0.5, 0.5, -0.5, 1.0),
    Vec4::new(0.5, 0.5, -0.5, 1.0),
    Vec4::new(-0.5, 0.5, -0.5, 1.0),
    Vec4::new(-0.5, -0.5, -0.5, 1.0),
    Vec4::new(-0.5, -0.5, 0.5, 1.0),
    Vec4::new(0.5, -0.5, 0.5, 1.0),
    Vec4::new(0.5, 0.5, 0.5, 1.0),
    Vec4::new(0.5, 0.5, 0.5, 1.0),
    Vec4::new(-0.5, 0.5, 0.5, 1.0),
    Vec4::new(-0.5, -0.5, 0.5, 1.0),
    Vec4::new(-0.5, 0.5, 0.5, 1.0),
    Vec4::new(-0.5, 0.5, -0.5, 1.0),
    Vec4::new(-0.5, -0.5, -0.5, 1.0),
    Vec4::new(-0.5, -0.5, -0.5, 1.0),
    Vec4::new(-0.5, -0.5, 0.5, 1.0),
    Vec4::new(-0.5, 0.5, 0.5, 1.0),
    Vec4::new(0.5, 0.5, 0.5, 1.0),
    Vec4::new(0.5, 0.5, -0.5, 1.0),
    Vec4::new(0.5, -0.5, -0.5, 1.0),
    Vec4::new(0.5, -0.5, -0.5, 1.0),
    Vec4::new(0.5, -0.5, 0.5, 1.0),
    Vec4::new(0.5, 0.5, 0.5, 1.0),
    Vec4::new(-0.5, -0.5, -0.5, 1.0),
    Vec4::new(0.5, -0.5, -0.5, 1.0),
    Vec4::new(0.5, -0.5, 0.5, 1.0),
    Vec4::new(0.5, -0.5, 0.5, 1.0),
    Vec4::new(-0.5, -0.5, 0.5, 1.0),
    Vec4::new(-0.5, -0.5, -0.5, 1.0),
    Vec4::new(-0.5, 0.5, -0.5, 1.0),
    Vec4::new(0.5, 0.5, -0.5, 1.0),
    Vec4::new(0.5, 0.5, 0.5, 1.0),
    Vec4::new(0.5, 0.5, 0.5, 1.0),
    Vec4::new(-0.5, 0.5, 0.5, 1.0),
    Vec4::new(-0.5, 0.5, -0.5, 1.0),
];

pub const CUBE_COLORS: &'static [Vec4] = &[
    Vec4::new(1.0, 0.0, 0.0, 1.0),
    Vec4::new(1.0, 0.0, 0.0, 1.0),
    Vec4::new(1.0, 0.0, 0.0, 1.0),
    Vec4::new(1.0, 0.0, 0.0, 1.0),
    Vec4::new(1.0, 0.0, 0.0, 1.0),
    Vec4::new(1.0, 0.0, 0.0, 1.0),
    Vec4::new(0.0, 1.0, 0.0, 1.0),
    Vec4::new(0.0, 1.0, 0.0, 1.0),
    Vec4::new(0.0, 1.0, 0.0, 1.0),
    Vec4::new(0.0, 1.0, 0.0, 1.0),
    Vec4::new(0.0, 1.0, 0.0, 1.0),
    Vec4::new(0.0, 1.0, 0.0, 1.0),
    Vec4::new(0.0, 0.0, 1.0, 1.0),
    Vec4::new(0.0, 0.0, 1.0, 1.0),
    Vec4::new(0.0, 0.0, 1.0, 1.0),
    Vec4::new(0.0, 0.0, 1.0, 1.0),
    Vec4::new(0.0, 0.0, 1.0, 1.0),
    Vec4::new(0.0, 0.0, 1.0, 1.0),
    Vec4::new(1.0, 0.0, 1.0, 1.0),
    Vec4::new(1.0, 0.0, 1.0, 1.0),
    Vec4::new(1.0, 0.0, 1.0, 1.0),
    Vec4::new(1.0, 0.0, 1.0, 1.0),
    Vec4::new(1.0, 0.0, 1.0, 1.0),
    Vec4::new(1.0, 0.0, 1.0, 1.0),
    Vec4::new(0.0, 1.0, 1.0, 1.0),
    Vec4::new(0.0, 1.0, 1.0, 1.0),
    Vec4::new(0.0, 1.0, 1.0, 1.0),
    Vec4::new(0.0, 1.0, 1.0, 1.0),
    Vec4::new(0.0, 1.0, 1.0, 1.0),
    Vec4::new(0.0, 1.0, 1.0, 1.0),
    Vec4::new(1.0, 1.0, 0.0, 1.0),
    Vec4::new(1.0, 1.0, 0.0, 1.0),
    Vec4::new(1.0, 1.0, 0.0, 1.0),
    Vec4::new(1.0, 1.0, 0.0, 1.0),
    Vec4::new(1.0, 1.0, 0.0, 1.0),
    Vec4::new(1.0, 1.0, 0.0, 1.0),
];

#[derive(SystemState)]
pub struct FrameRate {
    frame_ctr: usize,
    last_sec: Instant,
}

impl Default for FrameRate {
    fn default() -> Self {
        FrameRate {
            frame_ctr: 0,
            last_sec: Instant::now(),
        }
    }
}

impl FrameRate {
    fn pre_render(&mut self, _: PreRender, _: Commands, _: Queries<()>, _: Res<()>) {
        let now = Instant::now();
        self.frame_ctr += 1;
        if now.duration_since(self.last_sec).as_secs_f32() >= 1.0 {
            println!("Frame Rate: {}", self.frame_ctr);
            self.last_sec = now;
            self.frame_ctr = 0;
        }
    }
}

impl Into<System> for FrameRate {
    fn into(self) -> System {
        SystemBuilder::new(self)
            .with_handler(FrameRate::pre_render)
            .build()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Visualization {
    None,
    ClusterSlices,
    ShadowCascades,
    ClusterHeatMap,
}

pub struct Settings {
    pub slice_view_mat: Material,
    pub cluster_heatmap_mat: Material,
    pub cascade_view_mat: Material,
    pub visualization: Visualization,
}

impl View for Settings {
    fn show(
        &mut self,
        ctx: &egui::Context,
        _: &Commands,
        _: &Queries<Everything>,
        res: &Res<Everything>,
    ) {
        let mut settings = res.get_mut::<RendererSettings>().unwrap();

        egui::Window::new("Settings").show(ctx, |ui| {
            ui.add(
                egui::Slider::new(&mut settings.post_processing.exposure, 0.0..=1.0)
                    .step_by(0.01)
                    .text("Exposure"),
            );
            ui.toggle_value(&mut settings.post_processing.fxaa, "FXAA");
            ui.toggle_value(&mut settings.lock_occlusion, "Lock Occlusion");

            egui::ComboBox::from_label("Visualization")
                .selected_text(format!("{:?}", self.visualization))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.visualization, Visualization::None, "None");
                    ui.selectable_value(
                        &mut self.visualization,
                        Visualization::ClusterSlices,
                        "Cluster Slices",
                    );
                    ui.selectable_value(
                        &mut self.visualization,
                        Visualization::ClusterHeatMap,
                        "Cluster Heat Map",
                    );
                    ui.selectable_value(
                        &mut self.visualization,
                        Visualization::ShadowCascades,
                        "Shadow Cascades",
                    );
                });
        });

        settings.material_override = match &self.visualization {
            Visualization::None => None,
            Visualization::ClusterSlices => Some(self.slice_view_mat.clone()),
            Visualization::ClusterHeatMap => Some(self.cluster_heatmap_mat.clone()),
            Visualization::ShadowCascades => Some(self.cascade_view_mat.clone()),
        };
    }
}

#[allow(dead_code)]
fn main() {}
