use ard_engine::{
    assets::prelude::*, ecs::prelude::*, graphics::prelude::*, graphics_assets::prelude::*,
    input::*, math::*, window::prelude::*,
};

use crate::{
    scene_graph::SceneGraphAsset,
    util::{editor_job::EditorJob, ui::DragDropPayload},
};

use super::View;

#[derive(Default)]
pub struct SceneView {
    click_uv: Vec2,
    clicked: bool,
}

#[derive(Resource)]
pub struct SceneViewCamera {
    pub fov: f32,
    pub near: f32,
    pub far: f32,
    pub move_speed: f32,
    pub look_speed: f32,
    pub position: Vec3,
    pub rotation: Vec3,
}

impl Default for SceneViewCamera {
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
    #[inline]
    pub fn clicked(&self) -> bool {
        self.clicked
    }

    #[inline]
    pub fn click_uv(&self) -> Vec2 {
        self.click_uv
    }

    #[inline]
    pub fn reset_click(&mut self) {
        self.clicked = false;
    }
}

impl View for SceneView {
    fn show(
        &mut self,
        ui: &imgui::Ui,
        _controller: &mut crate::controller::Controller,
        resc: &mut crate::editor::Resources,
    ) {
        ui.window("Scene View").build(|| {
            // Draw the scene image
            let size = ui.content_region_avail();
            resc.renderer_settings.canvas_size =
                Some(((size[0] as u32).max(1), (size[1] as u32).max(1)));
            imgui::Image::new(DebugGui::scene_view(), size).build(ui);

            // Drag and drop for assets onto the scene
            if let Some(target) = ui.drag_drop_target() {
                if let Some(Ok(payload_data)) = target.accept_payload::<DragDropPayload, _>(
                    "Asset",
                    imgui::DragDropFlags::SOURCE_ALLOW_NULL_ID,
                ) {
                    // Fuggly, but we can make this nicer once if-let chains are stable
                    if payload_data.delivery {
                        if let DragDropPayload::Asset(handle) = payload_data.data {
                            if let Some(name) = resc.assets.get_name_by_id(handle.id) {
                                if let Some(ext) = name.extension() {
                                    match ext.to_str().unwrap() {
                                        <ModelAsset as Asset>::EXTENSION => {
                                            let handle = resc.assets.load::<ModelAsset>(&name);
                                            let assets_cl = resc.assets.clone();
                                            let commands_cl = resc.ecs_commands.entities.clone();
                                            let send = resc.scene_graph.new_node_channel();
                                            resc.jobs.add(EditorJob::new(
                                                "Instantiate Model",
                                                None,
                                                move || {
                                                    assets_cl.wait_for_load(&handle);

                                                    if let Some(model) = assets_cl.get(&handle) {
                                                        let node = crate::util::instantiate_model(
                                                            &model,
                                                            &handle,
                                                            &commands_cl,
                                                        );

                                                        let _ = send.send(node);
                                                    }
                                                },
                                                |ui| {
                                                    let style = unsafe { ui.style() };
                                                    ui.text("Loading...");
                                                    ui.same_line();
                                                    crate::util::ui::throbber(
                                                        ui,
                                                        8.0,
                                                        4.0,
                                                        8,
                                                        1.0,
                                                        style[imgui::StyleColor::Button],
                                                    );
                                                },
                                            ));
                                        }
                                        <SceneGraphAsset as Asset>::EXTENSION => {
                                            let handle = resc.assets.load::<SceneGraphAsset>(&name);
                                            let _ = resc
                                                .scene_graph
                                                .load_scene_channel()
                                                .send((handle, true));
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }

                target.pop();
            }

            // Transform camera
            let window = resc.windows.get_mut(WindowId::primary()).unwrap();
            if resc.input.mouse_button(MouseButton::Right) && ui.is_item_hovered() {
                let (mx, my) = resc.input.mouse_delta();
                resc.camera.rotation.x += (my as f32) * resc.camera.look_speed;
                resc.camera.rotation.y += (mx as f32) * resc.camera.look_speed;
                resc.camera.rotation.x = resc.camera.rotation.x.clamp(-85.0, 85.0);

                // Direction from rotation
                let rot = Mat4::from_euler(
                    EulerRot::YXZ,
                    resc.camera.rotation.y.to_radians(),
                    resc.camera.rotation.x.to_radians(),
                    0.0,
                );

                // Move camera
                let right = rot.col(0);
                let up = rot.col(1);
                let forward = rot.col(2);

                if resc.input.key(Key::W) {
                    resc.camera.position += forward.xyz() * resc.dt * resc.camera.move_speed;
                }

                if resc.input.key(Key::S) {
                    resc.camera.position -= forward.xyz() * resc.dt * resc.camera.move_speed;
                }

                if resc.input.key(Key::A) {
                    resc.camera.position -= right.xyz() * resc.dt * resc.camera.move_speed;
                }

                if resc.input.key(Key::D) {
                    resc.camera.position += right.xyz() * resc.dt * resc.camera.move_speed;
                }

                // Update camera
                let main_camera = resc.factory.main_camera();
                resc.factory.update_camera(
                    &main_camera,
                    CameraDescriptor {
                        position: resc.camera.position,
                        center: resc.camera.position + forward.xyz(),
                        up: up.xyz(),
                        near: resc.camera.near,
                        far: resc.camera.far,
                        fov: resc.camera.fov,
                    },
                );

                window.set_cursor_lock_mode(true);
            } else {
                window.set_cursor_lock_mode(false);
            }

            // Select entity in the view
            if resc.input.mouse_button_down(MouseButton::Left) && ui.is_item_hovered() {
                // Compute UV coordinate of the mouse position in scene view space
                let min = ui.item_rect_min();
                let mut max = ui.item_rect_max();
                let mut pos = resc.input.mouse_pos();

                pos.0 -= min[0] as f64;
                pos.1 -= min[1] as f64;
                max[0] -= min[0];
                max[1] -= min[1];

                self.click_uv = Vec2::new(
                    (pos.0 / max[0] as f64) as f32,
                    (pos.1 / max[1] as f64) as f32,
                );

                // Signal to the renderer that an entity image needs to be rendered
                resc.ecs_commands.events.submit(RenderEntityImage);
                self.clicked = true;
            }
        });
    }
}
