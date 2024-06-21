use ard_assets::manager::Assets;
use ard_core::{
    core::Tick,
    stat::{DirtyStatic, Static},
};
use ard_ecs::prelude::*;
use ard_render_assets::loader::{MaterialHandle, MeshHandle};

use crate::components::{destroy::Destroy, stat::MarkStatic};

use super::destroy::Destroyer;

#[derive(SystemState, Default)]
pub struct MarkStaticSystem;

impl MarkStaticSystem {
    fn tick(
        &mut self,
        _: Tick,
        commands: Commands,
        queries: Queries<(Read<MarkStatic>, Read<MeshHandle>, Read<MaterialHandle>)>,
        res: Res<(Read<DirtyStatic>, Read<Assets>)>,
    ) {
        let assets = res.get::<Assets>().unwrap();
        let mut dirty_flag = false;

        queries
            .filter()
            .without::<Static>()
            .without::<Destroy>()
            .make::<(
                Entity,
                (
                    Read<MarkStatic>,
                    Option<Read<MeshHandle>>,
                    Option<Read<MaterialHandle>>,
                ),
            )>()
            .into_iter()
            .for_each(|(entity, (_, mesh, material))| {
                if let Some(mesh) = mesh.and_then(|m| m.0.as_ref()) {
                    if assets.get(mesh).is_none() {
                        return;
                    }
                }

                if let Some(material) = material.and_then(|m| m.0.as_ref()) {
                    if assets.get(material).is_none() {
                        return;
                    }
                }

                commands.entities.add_component(entity, Static(0));
                dirty_flag = true;
            });

        dirty_flag |= queries
            .filter()
            .with::<Destroy>()
            .make::<Read<MarkStatic>>()
            .into_iter()
            .next()
            .is_some();

        if dirty_flag {
            let dirty_static = res.get::<DirtyStatic>().unwrap();
            dirty_static.signal(0);
        }
    }
}

impl From<MarkStaticSystem> for System {
    fn from(value: MarkStaticSystem) -> Self {
        SystemBuilder::new(value)
            .with_handler(MarkStaticSystem::tick)
            .run_before::<Tick, Destroyer>()
            .build()
    }
}
