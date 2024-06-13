use ard_engine::{
    ecs::prelude::*,
    game::components::transform::{Parent, Position, Rotation, Scale},
    math::*,
    render::{Camera, Model},
};
use transform_gizmo_egui::{math::Transform, prelude::*};

use crate::{camera::SceneViewCamera, selected::Selected};

use super::EditorViewContext;

#[derive(Default)]
pub struct TransformGizmo {
    gizmo: Gizmo,
    transform: Transform,
    selected: Option<Entity>,
    in_use: bool,
}

impl TransformGizmo {
    pub fn show(&mut self, ctx: &EditorViewContext, canvas_size: Vec2, canvas_rect: Rect) {
        let selected = ctx.res.get::<Selected>().unwrap();

        let selected_entity = match *selected {
            Selected::None => {
                self.in_use = false;
                self.selected = None;
                return;
            }
            Selected::Entity(e) => e,
        };

        if !ctx.queries.is_alive(selected_entity) {
            self.in_use = false;
            self.selected = None;
            return;
        }

        let scene_camera = ctx.res.get::<SceneViewCamera>().unwrap();
        let camera = ctx
            .queries
            .get::<Read<Camera>>(scene_camera.camera())
            .unwrap();
        let model = **ctx
            .queries
            .get::<Read<Model>>(scene_camera.camera())
            .unwrap();
        let gpu_struct = camera.into_gpu_struct(canvas_size.x, canvas_size.y, model);
        let proj = Mat4::perspective_lh(
            camera.fov,
            gpu_struct.aspect_ratio,
            gpu_struct.near_clip,
            gpu_struct.far_clip,
        );

        self.gizmo.update_config(GizmoConfig {
            view_matrix: gpu_struct.view.as_dmat4().into(),
            projection_matrix: proj.as_dmat4().into(),
            viewport: canvas_rect,
            modes: GizmoMode::all(),
            orientation: GizmoOrientation::Global,
            ..Default::default()
        });

        if Some(selected_entity) != self.selected {
            self.selected = Some(selected_entity);
            self.in_use = false;

            let object_model = ctx.queries.get::<Read<Model>>(selected_entity).unwrap();
            self.transform = Transform::from_scale_rotation_translation(
                object_model.scale().as_dvec3(),
                object_model.rotation().as_dquat(),
                object_model.position().as_dvec3(),
            );
        }

        let parent_model_inv = match ctx.queries.get::<Read<Parent>>(selected_entity) {
            Some(parent) => {
                let parent_model = ctx.queries.get::<Read<Model>>(parent.0).unwrap();
                parent_model.0.inverse()
            }
            None => Mat4::IDENTITY,
        };

        let mut position = ctx.queries.get::<Write<Position>>(selected_entity).unwrap();
        let mut rotation = ctx.queries.get::<Write<Rotation>>(selected_entity).unwrap();
        let mut scale = ctx.queries.get::<Write<Scale>>(selected_entity).unwrap();

        if let Some((r, t)) = self.gizmo.interact(ctx.ui, &[self.transform]) {
            self.in_use = true;

            let new_t = t[0];
            self.transform = new_t;

            let new_global_model = Mat4::from_scale_rotation_translation(
                DVec3::from(new_t.scale).as_vec3(),
                DQuat::from(new_t.rotation).as_quat(),
                DVec3::from(new_t.translation).as_vec3(),
            );

            let new_local_model = Model(parent_model_inv * new_global_model);

            match r {
                GizmoResult::Rotation { .. } | GizmoResult::Arcball { .. } => {
                    rotation.0 = new_local_model.rotation();
                }
                GizmoResult::Translation { .. } => {
                    position.0 = new_local_model.position();
                }
                GizmoResult::Scale { .. } => {
                    scale.0 = new_local_model.scale();
                }
            }
        }

        ctx.ui.input(|input| {
            if input.pointer.primary_released() && self.in_use {
                self.in_use = false;
            }
        });
    }
}
