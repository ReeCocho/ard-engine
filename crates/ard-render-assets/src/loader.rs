use ard_assets::{asset::AssetNameBuf, handle::Handle, manager::Assets};
use ard_core::core::Tick;
use ard_ecs::prelude::*;
use ard_render_base::RenderingMode;
use ard_render_material::material_instance::MaterialInstance;
use ard_render_meshes::mesh::Mesh;
use ard_save_load::{LoadContext, SaveContext, SaveLoad};
use serde::{Deserialize, Serialize};

use crate::{material::MaterialAsset, mesh::MeshAsset};

#[derive(Component)]
pub struct MeshHandle(pub Option<Handle<MeshAsset>>);

#[derive(Component)]
pub struct MaterialHandle(pub Option<Handle<MaterialAsset>>);

#[derive(SystemState)]
pub(crate) struct MeshLoaderSystem;

#[derive(SystemState)]
pub(crate) struct MaterialLoaderSystem;

#[derive(Serialize, Deserialize)]
pub struct SavedMeshHandle(pub Option<AssetNameBuf>);

#[derive(Serialize, Deserialize)]
pub struct SavedMaterialHandle(pub Option<AssetNameBuf>);

impl SaveLoad for MeshHandle {
    type Intermediate = SavedMeshHandle;

    fn load(ctx: &mut LoadContext, intermediate: Self::Intermediate) -> Self {
        MeshHandle(intermediate.0.and_then(|name| ctx.assets.load(&name)))
    }

    fn save(&self, ctx: &mut SaveContext) -> Self::Intermediate {
        SavedMeshHandle(self.0.as_ref().map(|handle| ctx.assets.get_name(&handle)))
    }
}

impl SaveLoad for MaterialHandle {
    type Intermediate = SavedMaterialHandle;

    fn load(ctx: &mut LoadContext, intermediate: Self::Intermediate) -> Self {
        MaterialHandle(intermediate.0.and_then(|name| ctx.assets.load(&name)))
    }

    fn save(&self, ctx: &mut SaveContext) -> Self::Intermediate {
        SavedMaterialHandle(self.0.as_ref().map(|handle| ctx.assets.get_name(&handle)))
    }
}

impl MeshLoaderSystem {
    fn tick(
        &mut self,
        _: Tick,
        commands: Commands,
        queries: Queries<(Write<MeshHandle>,)>,
        res: Res<(Read<Assets>,)>,
    ) {
        let assets = res.get::<Assets>().unwrap();
        queries
            .filter()
            .without::<Mesh>()
            .make::<(Entity, (Write<MeshHandle>,))>()
            .into_iter()
            .for_each(|(e, (handle,))| {
                let handle = match &handle.0 {
                    Some(handle) => handle,
                    None => return,
                };

                let asset = match assets.get(handle) {
                    Some(asset) => asset,
                    None => return,
                };
                commands.entities.add_component(e, asset.mesh.clone());
            });
    }
}

impl MaterialLoaderSystem {
    fn tick(
        &mut self,
        _: Tick,
        commands: Commands,
        queries: Queries<(Write<MaterialHandle>, Write<RenderingMode>)>,
        res: Res<(Read<Assets>,)>,
    ) {
        let assets = res.get::<Assets>().unwrap();
        queries
            .filter()
            .without::<MaterialInstance>()
            .make::<(
                Entity,
                (Write<MaterialHandle>, Option<Write<RenderingMode>>),
            )>()
            .into_iter()
            .for_each(|(e, (handle, render_mode))| {
                let handle = match &handle.0 {
                    Some(handle) => handle,
                    None => return,
                };

                let asset = match assets.get(handle) {
                    Some(asset) => asset,
                    None => return,
                };

                commands.entities.add_component(e, asset.instance.clone());
                match render_mode {
                    Some(render_mode) => {
                        *render_mode = asset.render_mode;
                    }
                    None => {
                        commands.entities.add_component(e, asset.render_mode);
                    }
                }
            });
    }
}

impl From<MeshLoaderSystem> for System {
    fn from(value: MeshLoaderSystem) -> Self {
        SystemBuilder::new(value)
            .with_handler(MeshLoaderSystem::tick)
            .build()
    }
}

impl From<MaterialLoaderSystem> for System {
    fn from(value: MaterialLoaderSystem) -> Self {
        SystemBuilder::new(value)
            .with_handler(MaterialLoaderSystem::tick)
            .build()
    }
}
