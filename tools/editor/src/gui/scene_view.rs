use ard_engine::{graphics::prelude::*, input::*, math::*, window::prelude::*};

pub struct SceneView {
    pub fov: f32,
    pub near: f32,
    pub far: f32,
    pub move_speed: f32,
    pub look_speed: f32,
    pub position: Vec3,
    pub rotation: Vec3,
}

impl Default for SceneView {
    fn default() -> Self {
        Self {
            near: 0.3,
            far: 300.0,
            fov: (100.0 as f32).to_radians(),
            look_speed: 0.1,
            move_speed: 30.0,
            position: Vec3::ZERO,
            rotation: Vec3::ZERO,
        }
    }
}

impl SceneView {
    pub fn draw(
        &mut self,
        dt: f32,
        factory: &Factory,
        input: &InputState,
        windows: &mut Windows,
        ui: &imgui::Ui,
        settings: &mut RendererSettings,
    ) {
        let mut opened = true;
        ui.show_demo_window(&mut opened);

        ui.window("Scene View").build(|| {
            // Draw the scene image
            let size = ui.content_region_avail();
            settings.canvas_size = Some(((size[0] as u32).max(1), (size[1] as u32).max(1)));
            imgui::Image::new(DebugGui::scene_view(), size).build(ui);

            // Transform camera
            let window = windows.get_mut(WindowId::primary()).unwrap();
            if input.mouse_button(MouseButton::Right) && ui.is_item_hovered() {
                let (mx, my) = input.mouse_delta();
                self.rotation.x += (my as f32) * self.look_speed;
                self.rotation.y += (mx as f32) * self.look_speed;
                self.rotation.x = self.rotation.x.clamp(-85.0, 85.0);

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
                    self.position += forward.xyz() * dt * self.move_speed;
                }

                if input.key(Key::S) {
                    self.position -= forward.xyz() * dt * self.move_speed;
                }

                if input.key(Key::A) {
                    self.position -= right.xyz() * dt * self.move_speed;
                }

                if input.key(Key::D) {
                    self.position += right.xyz() * dt * self.move_speed;
                }

                // Update camera
                let main_camera = factory.main_camera();
                factory.update_camera(
                    &main_camera,
                    CameraDescriptor {
                        position: self.position,
                        center: self.position + forward.xyz(),
                        up: up.xyz(),
                        near: self.near,
                        far: self.far,
                        fov: self.fov,
                    },
                );

                window.set_cursor_lock_mode(true);
            } else {
                window.set_cursor_lock_mode(false);
            }
        });
    }
}
