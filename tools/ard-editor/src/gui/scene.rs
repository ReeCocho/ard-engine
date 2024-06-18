use ard_engine::{
    assets::manager::Assets,
    ecs::prelude::*,
    game::components::transform::{Position, Rotation},
    input::{InputState, Key},
    math::*,
    render::{CanvasSize, Gui, SelectEntity},
};

use crate::{
    assets::meta::MetaData,
    camera::SceneViewCamera,
    selected::Selected,
    tasks::{instantiate::InstantiateTask, TaskQueue},
};

use super::{drag_drop::DragDropPayload, transform::TransformGizmo, EditorViewContext};

#[derive(Default)]
pub struct SceneView {
    gizmo: TransformGizmo,
    moving_time: f32,
}

impl SceneView {
    pub fn show(&mut self, ctx: EditorViewContext) -> egui_tiles::UiResponse {
        // Update the canvas size to match the viewport
        let canvas_size = ctx.ui.available_size_before_wrap();
        let origin = ctx.ui.cursor().left_top();
        ctx.res.get_mut::<CanvasSize>().unwrap().0 = Some((
            (canvas_size.x.ceil() as u32).max(1),
            (canvas_size.y.ceil() as u32).max(1),
        ));

        // Draw the scene view
        let scene_image = egui::Image::new(egui::ImageSource::Texture(egui::load::SizedTexture {
            id: Gui::SCENE_TEXTURE,
            size: canvas_size,
        }))
        .max_size(canvas_size)
        .fit_to_exact_size(canvas_size)
        .sense(egui::Sense {
            click: true,
            drag: true,
            focusable: true,
        });
        let (response, dnd) =
            ctx.ui
                .dnd_drop_zone::<DragDropPayload, _>(egui::Frame::none(), |ui| {
                    ui.add(scene_image.sense(egui::Sense {
                        click: true,
                        drag: true,
                        focusable: true,
                    }))
                });
        let response = response.inner;

        // See if anything was dragged onto the scene view that an be instantiated.
        if let Some(blarg) = dnd {
            let asset = match blarg.as_ref() {
                DragDropPayload::Asset(asset) => asset,
            };

            let assets = ctx.res.get::<Assets>().unwrap();
            let valid = match &asset.data {
                MetaData::Model => true,
            };

            if valid {
                let task_queue = ctx.res.get_mut::<TaskQueue>().unwrap();
                task_queue.add(InstantiateTask::new(asset.clone(), assets.clone()));
            }
        }

        // Entity selection
        if let Some(pos) = response.interact_pointer_pos() {
            if response.clicked() {
                *ctx.res.get_mut::<Selected>().unwrap() = Selected::None;
                let norm_pos = pos - origin;
                let uv = Vec2::new(
                    norm_pos.x.max(0.0) / canvas_size.x,
                    norm_pos.y.max(0.0) / canvas_size.y,
                );
                ctx.commands.events.submit(SelectEntity(uv));
            }
        }

        self.gizmo
            .show(&ctx, Vec2::new(canvas_size.x, canvas_size.y), response.rect);

        self.move_camera(&ctx, response);

        egui_tiles::UiResponse::None
    }

    fn move_camera(&mut self, ctx: &EditorViewContext, response: egui::Response) {
        let scene_camera = ctx.res.get::<SceneViewCamera>().unwrap();
        let input = ctx.res.get::<InputState>().unwrap();

        let mut query = ctx
            .queries
            .get::<(Write<Rotation>, Write<Position>)>(scene_camera.camera())
            .unwrap();

        let (ref mut rotation, ref mut position) = *query;

        if response.dragged_by(egui::PointerButton::Secondary) {
            let (mut ry, mut rx, rz) = rotation.0.to_euler(EulerRot::YXZ);

            rx += response.drag_delta().y * 0.007;
            ry += response.drag_delta().x * 0.007;
            rx = rx.clamp(
                -std::f32::consts::FRAC_PI_2 + 0.05,
                std::f32::consts::FRAC_PI_2 - 0.05,
            );

            rotation.0 = Quat::from_euler(EulerRot::YXZ, ry, rx, rz);

            // Direction from rotation
            let rot = Mat4::from_quat(rotation.0);

            // Move the camera
            let right = rot.col(0);
            let forward = rot.col(2);

            let dt = ctx.tick.0.as_secs_f32();
            let mut any_held = false;

            let mult = 8.0 + (self.moving_time.powf(3.0) * 2.0);
            if input.key(Key::W) {
                any_held = true;
                position.0 += Vec3A::from(forward.xyz() * dt * mult);
            }

            if input.key(Key::S) {
                any_held = true;
                position.0 -= Vec3A::from(forward.xyz() * dt * mult);
            }

            if input.key(Key::A) {
                any_held = true;
                position.0 -= Vec3A::from(right.xyz() * dt * mult);
            }

            if input.key(Key::D) {
                any_held = true;
                position.0 += Vec3A::from(right.xyz() * dt * mult);
            }

            if input.key(Key::Q) {
                any_held = true;
                position.0.y -= dt * mult;
            }

            if input.key(Key::E) {
                any_held = true;
                position.0.y += dt * mult;
            }

            if any_held {
                self.moving_time += ctx.tick.0.as_secs_f32();
            } else {
                self.moving_time = 0.0;
            }
        }

        if response.dragged_by(egui::PointerButton::Middle) {
            let rot = Mat4::from_quat(rotation.0);

            const SENSITIVITY: f32 = 0.1;
            let right = Vec3A::from(rot.col(0).xyz());
            let up = Vec3A::from(rot.col(1).xyz());
            let del = response.drag_delta() * SENSITIVITY;

            position.0 += (-right * del.x) + (up * del.y);
        }

        if response.hovered() {
            let yscroll = ctx.ui.input(|input| input.raw_scroll_delta.y);
            let rot = Mat4::from_quat(rotation.0);
            let forward = rot.col(2);

            position.0 += Vec3A::from(forward.xyz()) * (yscroll as f32);
        }
    }
}
