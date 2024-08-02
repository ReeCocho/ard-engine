use std::ops::Deref;

use ard_engine::{
    assets::prelude::*,
    core::core::Tick,
    ecs::prelude::*,
    render::{
        factory::Factory,
        loader::{MaterialHandle, MeshHandle},
        texture::TextureAsset,
        MaterialInstance, Mesh, TextureSlot,
    },
};
use rustc_hash::FxHashSet;

use crate::assets::meta::AssetType;

#[derive(Default, SystemState)]
pub struct RefresherSystem {
    materials: FxHashSet<AssetNameBuf>,
    meshes: FxHashSet<AssetNameBuf>,
    textures: FxHashSet<AssetNameBuf>,
}

#[derive(Clone, Event)]
pub struct RefreshAsset(pub AssetNameBuf);

impl RefresherSystem {
    fn tick(
        &mut self,
        _: Tick,
        commands: Commands,
        queries: Queries<(Write<MeshHandle>, Write<MaterialHandle>)>,
        res: Res<(Read<Assets>, Read<Factory>)>,
    ) {
        let assets = res.get::<Assets>().unwrap();
        let factory = res.get::<Factory>().unwrap();

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

        if !self.textures.is_empty() {
            for handle in queries.make::<Write<MaterialHandle>>() {
                let inner_handle = match &mut handle.0 {
                    Some(handle) => handle,
                    None => continue,
                };

                let mut mat_asset = match assets.get_mut(inner_handle) {
                    Some(asset) => asset,
                    None => continue,
                };

                let count = mat_asset.textures().len();
                for slot in 0..count {
                    let tex = match &mat_asset.textures()[slot] {
                        Some(tex) => tex,
                        None => continue,
                    };

                    let tex_name = assets.get_name(&tex);
                    if !self.textures.contains(&tex_name) {
                        continue;
                    }

                    let handle = match assets.load::<TextureAsset>(&tex_name) {
                        Some(handle) => handle,
                        None => continue,
                    };

                    let slot = TextureSlot::from(slot as u16);
                    mat_asset.set_texture(factory.deref(), slot, Some(handle));
                }
            }
            self.textures.clear();
        }
    }

    fn refresh_asset(&mut self, asset: RefreshAsset, _: Commands, _: Queries<()>, _: Res<()>) {
        let ty = match AssetType::try_from(asset.0.as_std_path()) {
            Ok(ty) => ty,
            Err(_) => return,
        };

        match ty {
            AssetType::Mesh => {
                self.meshes.insert(asset.0);
            }
            AssetType::Material => {
                self.materials.insert(asset.0);
            }
            AssetType::Texture => {
                self.textures.insert(asset.0);
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
