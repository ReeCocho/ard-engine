use ard_engine::{
    assets::prelude::*,
    core::core::Tick,
    ecs::prelude::*,
    render::{
        loader::{MaterialHandle, MeshHandle},
        MaterialInstance, Mesh,
    },
};
use rustc_hash::FxHashSet;

use crate::assets::meta::AssetType;

#[derive(Default, SystemState)]
pub struct RefresherSystem {
    materials: FxHashSet<AssetNameBuf>,
    meshes: FxHashSet<AssetNameBuf>,
}

#[derive(Clone, Event)]
pub struct RefreshAsset(pub AssetNameBuf);

impl RefresherSystem {
    fn tick(
        &mut self,
        _: Tick,
        commands: Commands,
        queries: Queries<(Write<MeshHandle>, Write<MaterialHandle>)>,
        res: Res<(Read<Assets>,)>,
    ) {
        let assets = res.get::<Assets>().unwrap();

        if !self.materials.is_empty() {
            for (entity, handle) in queries.make::<(Entity, Write<MaterialHandle>)>() {
                let inner_handle = match &mut handle.0 {
                    Some(handle) => handle,
                    None => continue,
                };

                let name = assets.get_name(inner_handle);
                if !self.materials.contains(&name) {
                    continue;
                }

                handle.0 = assets.load(&name);
                commands
                    .entities
                    .remove_component::<MaterialInstance>(entity);
            }
            self.materials.clear();
        }

        if !self.meshes.is_empty() {
            for (entity, handle) in queries.make::<(Entity, Write<MeshHandle>)>() {
                let inner_handle = match &mut handle.0 {
                    Some(handle) => handle,
                    None => continue,
                };

                let name = assets.get_name(inner_handle);
                if !self.meshes.contains(&name) {
                    continue;
                }

                handle.0 = assets.load(&name);
                commands.entities.remove_component::<Mesh>(entity);
            }
            self.meshes.clear();
        }
    }

    fn refresh_asset(&mut self, asset: RefreshAsset, _: Commands, _: Queries<()>, _: Res<()>) {
        let ty = match AssetType::try_from(asset.0.as_std_path()) {
            Ok(ty) => ty,
            Err(_) => return,
        };

        match ty {
            AssetType::Mesh => {
                self.meshes.insert(asset.0.clone());
            }
            AssetType::Material => {
                self.materials.insert(asset.0.clone());
            }
            _ => {}
        }
    }
}

impl From<RefresherSystem> for System {
    fn from(value: RefresherSystem) -> Self {
        SystemBuilder::new(value)
            .with_handler(RefresherSystem::tick)
            .with_handler(RefresherSystem::refresh_asset)
            .build()
    }
}
