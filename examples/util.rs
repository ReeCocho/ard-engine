use std::time::Instant;

use ard_core::prelude::Tick;
use ard_engine::{
    ecs::prelude::*,
    math::{EulerRot, Mat4, Vec3, Vec4},
};
use ard_input::{InputState, Key};
use ard_math::Vec4Swizzles;
use ard_render::{
    camera::{Camera, CameraDescriptor},
    factory::Factory,
    material::Material,
    renderer::{gui::View, PreRender, RendererSettings},
    static_geometry::StaticRenderableHandle,
};
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

#[derive(Component)]
pub struct MainCamera(pub Camera);

#[derive(Resource)]
pub struct StaticHandles(pub Vec<StaticRenderableHandle>);

#[derive(SystemState)]
pub struct CameraMover {
    pub cursor_locked: bool,
    pub look_speed: f32,
    pub move_speed: f32,
    pub entity: Entity,
    pub position: Vec3,
    pub rotation: Vec3,
    pub descriptor: CameraDescriptor,
}

impl CameraMover {
    fn on_tick(
        &mut self,
        evt: Tick,
        _: Commands,
        queries: Queries<(Write<MainCamera>,)>,
        res: Res<(Read<Factory>, Read<InputState>, Write<Windows>)>,
    ) {
        let factory = res.get::<Factory>().unwrap();
        let input = res.get::<InputState>().unwrap();
        let mut windows = res.get_mut::<Windows>().unwrap();
        let main_camera = queries.get::<Write<MainCamera>>(self.entity).unwrap();

        // Rotate the camera
        let delta = evt.0.as_secs_f32();
        if self.cursor_locked {
            let (mx, my) = input.mouse_delta();
            self.rotation.x += (my as f32) * self.look_speed;
            self.rotation.y += (mx as f32) * self.look_speed;
            self.rotation.x = self.rotation.x.clamp(-85.0, 85.0);
        }

        // Direction from rotation
        let rot = Mat4::from_euler(
            EulerRot::YXZ,
            self.rotation.y.to_radians(),
            self.rotation.x.to_radians(),
            0.0,
        );

        // Move the camera
        let right = rot.col(0);
        let up = rot.col(1);
        let forward = rot.col(2);

        if self.cursor_locked {
            if input.key(Key::W) {
                self.position += forward.xyz() * delta * self.move_speed;
            }

            if input.key(Key::S) {
                self.position -= forward.xyz() * delta * self.move_speed;
            }

            if input.key(Key::A) {
                self.position -= right.xyz() * delta * self.move_speed;
            }

            if input.key(Key::D) {
                self.position += right.xyz() * delta * self.move_speed;
            }
        }

        // Lock cursor
        if input.key_up(Key::M) {
            self.cursor_locked = !self.cursor_locked;

            let window = windows.get_mut(WindowId::primary()).unwrap();

            window.set_cursor_lock_mode(self.cursor_locked);
            window.set_cursor_visibility(!self.cursor_locked);
        }

        // Toggle AO
        if input.key_up(Key::O) {
            self.descriptor.ao = !self.descriptor.ao;
        }

        // Update the camera
        self.descriptor.position = self.position;
        self.descriptor.target = self.position + forward.xyz();
        self.descriptor.up = up.xyz();
        self.descriptor.near = 0.1;
        self.descriptor.far = 150.0;
        factory.update_camera(&main_camera.0, self.descriptor.clone());
    }
}

impl From<CameraMover> for System {
    fn from(mover: CameraMover) -> Self {
        SystemBuilder::new(mover)
            .with_handler(CameraMover::on_tick)
            .build()
    }
}

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
