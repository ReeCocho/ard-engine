use ard_engine::{graphics::prelude::*, math::*};

use crate::gui::scene_view::SceneView;

const GIZMO_SCALE_FACTOR: f32 = 0.25;

pub struct TransformGizmo {}

impl TransformGizmo {
    pub fn draw(&self, drawing: &DebugDrawing, view: &SceneView, model: Mat4) {
        let pos = model.col(3).xyz();

        let scale = Vec3::new(
            model.col(0).xyz().length() * model.col(0).x.signum(),
            model.col(1).xyz().length() * model.col(1).y.signum(),
            model.col(2).xyz().length() * model.col(2).z.signum(),
        );

        let rot = Mat4::from_cols(
            if scale.x == 0.0 {
                Vec4::X
            } else {
                Vec4::from((model.col(0).xyz() / scale.x, 0.0))
            },
            if scale.y == 0.0 {
                Vec4::Y
            } else {
                Vec4::from((model.col(1).xyz() / scale.y, 0.0))
            },
            if scale.z == 0.0 {
                Vec4::Z
            } else {
                Vec4::from((model.col(2).xyz() / scale.z, 0.0))
            },
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        );

        let model = Mat4::from_translation(pos)
            * rot
            * Mat4::from_scale(Vec3::ONE * (pos - view.position).length() * GIZMO_SCALE_FACTOR);

        let origin = (model * Vec4::new(0.0, 0.0, 0.0, 1.0)).xyz();
        let x = (model * Vec4::from((Vec3::X, 1.0))).xyz();
        let y = (model * Vec4::from((Vec3::Y, 1.0))).xyz();
        let z = (model * Vec4::from((Vec3::Z, 1.0))).xyz();

        // X-axis
        drawing.draw_line(origin, x, Vec3::X);
        draw_translate_tip(
            drawing,
            Vec3::X,
            Vec3::new(0.0, 0.0, -std::f32::consts::FRAC_PI_2),
            model,
        );

        // Y-axis
        drawing.draw_line(origin, y, Vec3::Y);
        draw_translate_tip(drawing, Vec3::Y, Vec3::ZERO, model);

        // Z-axis
        drawing.draw_line(origin, z, Vec3::Z);
        draw_translate_tip(
            drawing,
            Vec3::Z,
            Vec3::new(std::f32::consts::FRAC_PI_2, 0.0, 0.0),
            model,
        );
    }
}

fn draw_translate_tip(drawing: &DebugDrawing, color: Vec3, rotation: Vec3, model: Mat4) {
    const TIP_SIZE: f32 = 0.07;

    // Three lines for the triangle base and three more for the tip
    let mut points = [(Vec3::ZERO, Vec3::ZERO); 6];

    // Base
    for i in 0..3 {
        let ang1 = (i as f32 * 120.0) * std::f32::consts::PI / 180.0;
        let ang2 = ((i + 1) as f32 * 120.0) * std::f32::consts::PI / 180.0;

        points[i].0 = Vec3::new(ang1.cos(), 0.0, ang1.sin()) * TIP_SIZE;

        points[i].1 = Vec3::new(ang2.cos(), 0.0, ang2.sin()) * TIP_SIZE;
    }

    // Tip
    for i in 3..6 {
        let ang1 = (i as f32 * 120.0) * std::f32::consts::PI / 180.0;

        points[i].0 = Vec3::new(ang1.cos(), 0.0, ang1.sin()) * TIP_SIZE;

        points[i].1 = Vec3::new(0.0, 3.0, 0.0) * TIP_SIZE;
    }

    // Apply model matrix and then draw
    let model = model
        * Mat4::from_euler(EulerRot::XYZ, rotation.x, rotation.y, rotation.z)
        * Mat4::from_translation(Vec3::Y);
    for pt in &mut points {
        pt.0 = (model * Vec4::from((pt.0, 1.0))).xyz();
        pt.1 = (model * Vec4::from((pt.1, 1.0))).xyz();
        drawing.draw_line(pt.0, pt.1, color);
    }
}
