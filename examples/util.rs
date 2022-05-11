use ard_engine::{
    core::prelude::*,
    ecs::prelude::*,
    graphics::prelude::*,
    input::*,
    math::{EulerRot, Mat4, Vec3},
    window::prelude::*,
};
use ard_math::Vec4Swizzles;

use std::time::Instant;

pub struct FrameRate {
    frame_ctr: usize,
    last_sec: Instant,
}

pub struct CameraMovement {
    pub fov: f32,
    pub near: f32,
    pub far: f32,
    pub move_speed: f32,
    pub look_speed: f32,
    pub position: Vec3,
    pub rotation: Vec3,
    pub cursor_locked: bool,
}

#[derive(Resource, Default)]
pub struct MainCameraState(pub CameraDescriptor);

impl SystemState for FrameRate {}

impl SystemState for CameraMovement {}

type CameraMovementRes = (
    Read<Factory>,
    Read<InputState>,
    Write<Windows>,
    Read<DebugDrawing>,
    Write<MainCameraState>,
);

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

impl CameraMovement {
    fn tick(
        &mut self,
        tick: Tick,
        _: Commands,
        _: Queries<(Read<Model>, Read<PointLight>)>,
        res: Res<CameraMovementRes>,
    ) {
        let res = res.get();
        let factory = res.0.unwrap();
        let input = res.1.unwrap();
        let mut windows = res.2.unwrap();
        let debug_drawing = res.3.unwrap();
        let mut camera_state = res.4.unwrap();

        let main_camera = factory.main_camera();

        let delta = tick.0.as_secs_f32();

        // Rotate camera
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

        // Move camera
        let right = rot.col(0);
        let up = rot.col(1);
        let forward = rot.col(2);

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

        // Update camera
        camera_state.0 = CameraDescriptor {
            position: self.position,
            center: self.position + forward.xyz(),
            up: up.xyz(),
            near: self.near,
            far: self.far,
            fov: self.fov,
        };

        // Lock cursor
        if input.key_down(Key::M) {
            self.cursor_locked = !self.cursor_locked;

            let window = windows.get_mut(WindowId::primary()).unwrap();

            window.set_cursor_lock_mode(self.cursor_locked);
            window.set_cursor_visibility(!self.cursor_locked);
        }
    }

    fn pre_render(
        &mut self,
        _: PreRender,
        _: Commands,
        _: Queries<()>,
        res: Res<CameraMovementRes>,
    ) {
        let res = res.get();
        let factory = res.0.unwrap();
        let debug_drawing = res.3.unwrap();
        let camera_state = res.4.unwrap();

        let main_camera = factory.main_camera();

        factory.update_camera(&main_camera, camera_state.0);

        // Debug frustum
        debug_drawing.draw_frustum(camera_state.0, Vec3::new(1.0, 1.0, 1.0));
    }
}

impl Into<System> for FrameRate {
    fn into(self) -> System {
        SystemBuilder::new(self)
            .with_handler(FrameRate::pre_render)
            .build()
    }
}

impl Into<System> for CameraMovement {
    fn into(self) -> System {
        SystemBuilder::new(self)
            .with_handler(CameraMovement::tick)
            .with_handler(CameraMovement::pre_render)
            .build()
    }
}

#[allow(dead_code)]
fn main() {}
