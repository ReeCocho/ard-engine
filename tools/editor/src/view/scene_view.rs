use ard_engine::{
    assets::prelude::*,
    ecs::prelude::*,
    game::components::transform::{Parent, Transform},
    graphics::prelude::*,
    graphics_assets::prelude::*,
    input::*,
    math::*,
    window::prelude::*,
};

use crate::{
    controller::Command,
    scene_graph::SceneGraphAsset,
    util::{editor_job::EditorJob, ui::DragDropPayload},
};

use super::{inspector::GizmoAxis, View};

#[derive(Default)]
pub struct SceneView {
    click_uv: Vec2,
    clicked: bool,
    gizmo_manip: Option<GizmoManipulation>,
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
    pub mouse_ray: Vec3,
    pub view: Mat4,
    pub projection: Mat4,
    pub min: Vec2,
    pub max: Vec2,
    pub gizmo_axis: Option<(GizmoAxis, Entity)>,
}

struct TransformModify {
    entity: Entity,
    old: Transform,
    new: Transform,
}

struct GizmoManipulation {
    axis: GizmoAxis,
    entity: Entity,
    old_transform: Transform,
    old_position: Vec3,
    old_rotation: Mat4,
    old_scale: Vec3,
}

impl Default for SceneViewCamera {
    fn default() -> Self {
        Self {
            near: 0.3,
            far: 500.0,
            fov: (100.0 as f32).to_radians(),
            look_speed: 0.1,
            move_speed: 30.0,
            position: Vec3::ZERO,
            rotation: Vec3::ZERO,
            mouse_ray: Vec3::Z,
            view: Mat4::IDENTITY,
            projection: Mat4::IDENTITY,
            min: Vec2::ONE,
            max: Vec2::ONE,
            gizmo_axis: None,
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
        controller: &mut crate::controller::Controller,
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

            // Compute UV coordinate of the mouse position in scene view space
            let min = ui.item_rect_min();
            let mut max = ui.item_rect_max();
            let mut pos = resc.input.mouse_pos();

            resc.camera.min = Vec2::new(min[0] as f32, min[1] as f32);
            resc.camera.max = Vec2::new(max[0] as f32, max[1] as f32);

            pos.0 -= min[0] as f64;
            pos.1 -= min[1] as f64;
            max[0] -= min[0];
            max[1] -= min[1];

            self.click_uv = Vec2::new(
                (pos.0 / max[0] as f64) as f32,
                (pos.1 / max[1] as f64) as f32,
            )
            .clamp(Vec2::ZERO, Vec2::ONE);

            let window = resc.windows.get_mut(WindowId::primary()).unwrap();
            if ui.is_item_hovered() {
                // Transform camera
                if resc.input.mouse_button(MouseButton::Right) {
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

                    // Compute view and projection for scene view
                    resc.camera.view = Mat4::look_at_lh(
                        resc.camera.position,
                        resc.camera.position + forward.xyz(),
                        up.xyz(),
                    );
                    resc.camera.projection = Mat4::perspective_lh(
                        resc.camera.fov,
                        max[0] as f32 / max[1] as f32,
                        resc.camera.near,
                        resc.camera.far,
                    );

                    window.set_cursor_lock_mode(true);
                } else {
                    window.set_cursor_lock_mode(false);
                }

                match &self.gizmo_manip {
                    Some(gizmo) => {
                        if resc.input.mouse_button(MouseButton::Left) {
                            // Find the axis' of the object
                            let obj_right = gizmo.old_rotation.col(0).xyz().normalize();
                            let obj_up = gizmo.old_rotation.col(1).xyz().normalize();
                            let obj_forward = gizmo.old_rotation.col(2).xyz().normalize();

                            // Find the axis' of the camera
                            let rot = Mat4::from_euler(
                                EulerRot::YXZ,
                                resc.camera.rotation.y.to_radians(),
                                resc.camera.rotation.x.to_radians(),
                                0.0,
                            );
                            let cam_right = rot.col(0).xyz().normalize();
                            let cam_up = rot.col(1).xyz().normalize();

                            let mut transform =
                                resc.queries.get::<Write<Transform>>(gizmo.entity).unwrap();

                            let parent_model =
                                match resc.queries.get::<Read<Parent>>(gizmo.entity).unwrap().0 {
                                    Some(parent) => {
                                        resc.queries.get::<Read<Model>>(parent).unwrap().0
                                    }
                                    None => Mat4::IDENTITY,
                                };

                            // Compute the global transformation of the object
                            let mut global_position =
                                (parent_model * Vec4::from((transform.position, 1.0))).xyz();

                            // Find and scale the input axis'
                            let (m_dx, m_dy) = resc.input.mouse_delta();
                            let mdel = Vec2::new(m_dx as f32, -m_dy as f32)
                                * resc.camera.position.distance(global_position)
                                * 0.002;

                            resc.debug_draw.draw_line(
                                transform.position(),
                                transform.position() + obj_right,
                                Vec3::ONE,
                            );

                            match gizmo.axis {
                                GizmoAxis::X => {
                                    let scale = (obj_right.dot(cam_right) * mdel.x)
                                        + (obj_right.dot(cam_up) * mdel.y);
                                    global_position += obj_right * scale;
                                    transform.position = Vec3A::from(
                                        (parent_model.inverse()
                                            * Vec4::from((global_position, 1.0)))
                                        .xyz(),
                                    );
                                }
                                GizmoAxis::Y => {
                                    let scale = (obj_up.dot(cam_right) * mdel.x)
                                        + (obj_up.dot(cam_up) * mdel.y);
                                    global_position += obj_up * scale;
                                    transform.position = Vec3A::from(
                                        (parent_model.inverse()
                                            * Vec4::from((global_position, 1.0)))
                                        .xyz(),
                                    );
                                }
                                GizmoAxis::Z => {
                                    let scale = (obj_forward.dot(cam_right) * mdel.x)
                                        + (obj_forward.dot(cam_up) * mdel.y);
                                    global_position += obj_forward * scale;
                                    transform.position = Vec3A::from(
                                        (parent_model.inverse()
                                            * Vec4::from((global_position, 1.0)))
                                        .xyz(),
                                    );
                                }
                                _ => todo!(),
                            }
                        } else {
                            // Submit command for undo/redo
                            let transform =
                                resc.queries.get::<Read<Transform>>(gizmo.entity).unwrap();
                            controller.submit(TransformModify {
                                entity: gizmo.entity,
                                old: gizmo.old_transform,
                                new: transform.clone(),
                            });
                            self.gizmo_manip = None;
                        }
                    }
                    None => {
                        if resc.input.mouse_button_down(MouseButton::Left) {
                            match resc.camera.gizmo_axis {
                                // Axis is hovered so we need to transform it
                                Some((axis, entity)) => {
                                    let old_transform = resc
                                        .queries
                                        .get::<Read<Transform>>(entity)
                                        .unwrap()
                                        .clone();

                                    let model =
                                        resc.queries.get::<Read<Model>>(entity).unwrap().0.clone();

                                    let (old_position, old_rotation, old_scale) =
                                        crate::util::extract_transformations(model);

                                    self.gizmo_manip = Some(GizmoManipulation {
                                        axis,
                                        entity,
                                        old_transform,
                                        old_position,
                                        old_rotation,
                                        old_scale,
                                    });
                                }
                                // No axis is hovered so we are selecting something in the view and need an
                                // entity image
                                None => {
                                    resc.ecs_commands.events.submit(RenderEntityImage);
                                    self.clicked = true;
                                }
                            }
                        }
                    }
                }
            }
        });
    }
}

impl SceneViewCamera {
    /// Given a screen space pixel coordinate, determine a world space position on the near plane.
    #[inline]
    pub fn screen_to_world(&self, mut uv: Vec2) -> Vec3 {
        uv -= self.min;
        uv /= self.max - self.min;
        uv *= 2.0;
        uv -= Vec2::ONE;
        uv.y = -uv.y;
        let res = (self.projection * self.view).inverse() * Vec4::new(uv.x, uv.y, 0.0, 1.0);
        res.xyz() / res.w
    }
}

impl Command for TransformModify {
    fn undo(&mut self, resc: &mut crate::editor::Resources) {
        let mut transform = resc.queries.get::<Write<Transform>>(self.entity).unwrap();
        **transform = self.old;
    }

    fn redo(&mut self, resc: &mut crate::editor::Resources) {
        let mut transform = resc.queries.get::<Write<Transform>>(self.entity).unwrap();
        **transform = self.new;
    }
}
