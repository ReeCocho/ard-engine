pub use ard_engine::ecs::prelude::*;
use ard_engine::{
    core::core::Tick,
    input::{InputState, Key},
    math::{Vec4, Vec4Swizzles},
    render::{Camera, Mesh},
    transform::{Model, Position},
};

use crate::{camera::SceneViewCamera, selected::Selected};

const SHLOOP_SPEED: f32 = 4.0;

/// System that allows you to move the camera to the selected entity.
#[derive(SystemState, Default)]
pub struct Shlooper {
    target: Option<Entity>,
    lerp_factor: f32,
}

type ShlooperQueries = (Read<Mesh>, Read<Model>, Read<Camera>, Write<Position>);

impl Shlooper {
    fn tick(
        &mut self,
        tick: Tick,
        _: Commands,
        queries: Queries<ShlooperQueries>,
        res: Res<(Read<Selected>, Read<SceneViewCamera>, Read<InputState>)>,
    ) {
        let camera = res.get::<SceneViewCamera>().unwrap().camera();

        self.shloop_to_target(tick, camera, &queries);

        let selected = match *res.get::<Selected>().unwrap() {
            Selected::Entity(entity) => entity,
            _ => return,
        };

        if res.get::<InputState>().unwrap().key_down(Key::F) {
            self.target = Some(selected);
            self.lerp_factor = 0.0;
        }
    }

    fn shloop_to_target(&mut self, tick: Tick, camera: Entity, queries: &Queries<ShlooperQueries>) {
        let target = match self.target {
            Some(target) => target,
            None => return,
        };

        let target_bounds = queries
            .get::<Read<Mesh>>(target)
            .map(|mesh| mesh.bounds())
            .unwrap_or_default();
        let mut target_bounding_sphere = target_bounds.bounding_sphere();
        let target_model = queries
            .get::<Read<Model>>(target)
            .map(|m| m.clone())
            .unwrap_or(Model::default());

        let mut targets_position = target_model.0 * Vec4::from((target_bounding_sphere.xyz(), 1.0));
        targets_position /= targets_position.w;

        let target_scale_max = target_model.scale().abs().max_element();
        target_bounding_sphere.w = 1.5 * (target_bounding_sphere.w * target_scale_max).max(1.0);

        self.lerp_factor = (self.lerp_factor + (tick.0.as_secs_f32() * SHLOOP_SPEED)).min(1.0);

        let camera_model = *queries.get::<Read<Model>>(camera).unwrap();
        let camera_fov = queries.get::<Read<Camera>>(camera).unwrap().fov * 0.5;
        let camera_forward = camera_model.forward();

        let hypot = target_bounding_sphere.w / camera_fov.sin().max(0.01);
        let dist = hypot * camera_fov.cos().max(0.01);

        let goto_position = targets_position.xyz() - (camera_forward * dist);

        let mut camera_position = queries.get::<Write<Position>>(camera).unwrap();
        camera_position.0 = camera_position
            .0
            .lerp(goto_position.into(), self.lerp_factor.powf(5.0));

        if self.lerp_factor >= 1.0 {
            self.target = None;
            self.lerp_factor = 0.0;
        }
    }
}

impl From<Shlooper> for System {
    fn from(value: Shlooper) -> Self {
        SystemBuilder::new(value)
            .with_handler(Shlooper::tick)
            .build()
    }
}
