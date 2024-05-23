use ard_engine::{
    ecs::prelude::*,
    game::components::transform::Rotation,
    math::{EulerRot, Quat},
    render::{CanvasSize, Gui},
};

use crate::camera::SceneViewCamera;

use super::EditorViewContext;

pub struct SceneView {}

impl SceneView {
    pub fn show(&mut self, ctx: EditorViewContext) -> egui_tiles::UiResponse {
        let canvas_size = ctx.ui.available_size_before_wrap();
        ctx.res.get_mut::<CanvasSize>().unwrap().0 = Some((
            (canvas_size.x.ceil() as u32).max(1),
            (canvas_size.y.ceil() as u32).max(1),
        ));

        let scene_image = egui::Image::new(egui::ImageSource::Texture(egui::load::SizedTexture {
            id: Gui::SCENE_TEXTURE,
            size: canvas_size,
        }))
        .max_size(canvas_size)
        .fit_to_exact_size(canvas_size)
        .sense(egui::Sense {
            click: false,
            drag: true,
            focusable: true,
        });
        let response = ctx.ui.add(scene_image);

        if response.dragged_by(egui::PointerButton::Secondary) {
            let scene_camera = ctx.res.get::<SceneViewCamera>().unwrap();

            let mut rotation = ctx
                .queries
                .get::<Write<Rotation>>(scene_camera.camera())
                .unwrap();
            let (mut ry, mut rx, rz) = rotation.0.to_euler(EulerRot::YXZ);

            rx += response.drag_delta().y * 0.01;
            ry += response.drag_delta().x * 0.01;
            rx = rx.clamp(
                -std::f32::consts::FRAC_PI_2 + 0.05,
                std::f32::consts::FRAC_PI_2 - 0.05,
            );

            rotation.0 = Quat::from_euler(EulerRot::YXZ, ry, rx, rz);
        }

        egui_tiles::UiResponse::None
    }
}
