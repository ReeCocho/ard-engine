use std::ops::DerefMut;

use ard_engine::{
    core::stat::Static,
    ecs::prelude::*,
    game::components::transform::{Parent, Position, Rotation, Scale},
    math::*,
    render::{Camera, Model},
};
use transform_gizmo_egui::{math::Transform, prelude::*};

use crate::{camera::SceneViewCamera, inspect::transform::EulerRotation, selected::Selected};

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

        if ctx.queries.get::<Read<Model>>(selected_entity).is_none() {
            self.in_use = false;
            self.selected = None;
            return;
        }

        if ctx.queries.get::<Read<Static>>(selected_entity).is_some() {
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

        let parent_model = match ctx.queries.get::<Read<Parent>>(selected_entity) {
            Some(parent) => ctx.queries.get::<Read<Model>>(parent.0).unwrap().0,
            None => Mat4::IDENTITY,
        };
        let parent_model_inv = parent_model.inverse();

        let model = *ctx.queries.get::<Read<Model>>(selected_entity).unwrap();
        let mut position = ctx.queries.get::<Write<Position>>(selected_entity).unwrap();
        let mut rotation = ctx
            .queries
            .get::<(Entity, Write<Rotation>, Write<EulerRotation>)>(selected_entity)
            .unwrap();
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
                    let (rotation, euler_rot) = rotation.deref_mut();
                    rotation.0 = new_local_model.rotation();
                    if let Some(euler_rot) = euler_rot {
                        let (y, x, z) = rotation.0.to_euler(EulerRot::YXZ);
                        euler_rot.0 = Vec3A::new(x.to_degrees(), y.to_degrees(), z.to_degrees());
                    }
                }
                GizmoResult::Translation { .. } => {
                    position.0 = new_local_model.position();
                }
                GizmoResult::Scale { .. } => {
                    scale.0 = new_local_model.scale();
                }
            }
        }
        // If the transform changed, but it wasn't from the gizmo, we need to apply transformations
        else {
            let our_pos = DVec3::from(self.transform.translation).as_vec3a();
            let our_rot = DQuat::from(self.transform.rotation).as_quat();
            let our_scl = DVec3::from(self.transform.scale).as_vec3a();

            let pos = model.position();
            let rot = model.rotation();
            let scl = model.scale();

            let del = (our_pos - pos).length();
            if del > 0.01 {
                self.transform.translation = pos.as_dvec3().into();
            }

            if our_rot.angle_between(rot) > 0.01 {
                let (_, euler_rot) = rotation.deref_mut();
                self.transform.rotation = rot.as_dquat().into();
                if let Some(euler_rot) = euler_rot {
                    let (y, x, z) = rot.to_euler(EulerRot::YXZ);
                    euler_rot.0 = Vec3A::new(x.to_degrees(), y.to_degrees(), z.to_degrees());
                }
            }

            if (our_scl - scl).length() > 0.01 {
                self.transform.scale = scl.as_dvec3().into();
            }
        }

        ctx.ui.input(|input| {
            if input.pointer.primary_released() && self.in_use {
                self.in_use = false;
            }
        });
    }
}
