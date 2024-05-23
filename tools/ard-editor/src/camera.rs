use ard_engine::{
    core::prelude::*,
    ecs::prelude::*,
    game::components::transform::{Position, Rotation, Scale},
    math::Vec4,
    render::{Camera, CameraClearColor, Model, RenderFlags},
};

#[derive(Resource)]
pub struct SceneViewCamera {
    camera: Entity,
}

impl SceneViewCamera {
    pub fn new(app: &App) -> Self {
        let mut entity = [Entity::null()];
        app.world.entities().commands().create(
            (
                vec![Position::default()],
                vec![Rotation::default()],
                vec![Scale::default()],
                vec![Model::default()],
                vec![Camera {
                    near: 0.03,
                    far: 300.0,
                    fov: 80.0_f32.to_radians(),
                    order: 0,
                    clear_color: CameraClearColor::Color(Vec4::ZERO),
                    flags: RenderFlags::empty(),
                }],
            ),
            &mut entity,
        );

        Self { camera: entity[0] }
    }

    #[inline(always)]
    pub fn camera(&self) -> Entity {
        self.camera
    }
}
