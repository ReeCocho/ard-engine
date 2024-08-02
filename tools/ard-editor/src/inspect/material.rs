use std::any::TypeId;

use ard_engine::{
    assets::prelude::Assets,
    ecs::prelude::*,
    render::{loader::MaterialHandle, material::MaterialAsset, MaterialInstance, RenderingMode},
};

use crate::{assets::meta::AssetType, gui::util};

use super::Inspector;

pub struct MaterialInspector;

impl Inspector for MaterialInspector {
    fn should_inspect(&self, ctx: super::InspectorContext) -> bool {
        ctx.queries
            .component_types(ctx.entity)
            .contains(TypeId::of::<MaterialHandle>())
    }

    fn title(&self) -> &'static str {
        "Material"
    }

    fn show(&mut self, ctx: super::InspectorContext) {
        let assets = ctx.res.get::<Assets>().unwrap();
        let mut handle = ctx
            .queries
            .get::<Write<MaterialHandle>>(ctx.entity)
            .unwrap();

        let _changed = util::drag_drop_asset_target(
            ctx.ui,
            handle
                .0
                .as_ref()
                .map(|h| assets.get_name(h))
                .unwrap_or_default(),
            |asset| match AssetType::try_from(asset.meta_file().baked.as_std_path()) {
                Ok(AssetType::Material) => true,
                _ => false,
            },
            |asset| {
                handle.0 =
                    asset.and_then(|asset| assets.load::<MaterialAsset>(&asset.meta_file().baked));
                ctx.commands
                    .entities
                    .remove_component::<MaterialInstance>(ctx.entity);
                true
            },
        );
    }

    fn remove(&mut self, ctx: super::InspectorContext) {
        ctx.commands
            .entities
            .remove_component::<MaterialInstance>(ctx.entity);
        ctx.commands
            .entities
            .remove_component::<MaterialHandle>(ctx.entity);
        ctx.commands
            .entities
            .remove_component::<RenderingMode>(ctx.entity);
    }
}
