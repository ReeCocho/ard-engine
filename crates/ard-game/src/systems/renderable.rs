use ard_assets::manager::Assets;
use ard_ecs::prelude::*;
use ard_render::renderer::{PreRender, RenderLayer, Renderable};

use crate::components::renderable::{RenderableData, RenderableSource};

#[derive(SystemState)]
pub struct ApplyRenderableData;

impl ApplyRenderableData {
    pub fn on_pre_render(
        &mut self,
        _: PreRender,
        commands: Commands,
        queries: Queries<Read<RenderableData>>,
        res: Res<(Read<Assets>,)>,
    ) {
        let assets = res.get::<Assets>().unwrap();

        // Find every entity that has renderable data but no active renderer
        for (entity, data) in queries
            .filter()
            .without::<Renderable>()
            .make::<(Entity, Read<RenderableData>)>()
        {
            // Check to see if the renderable data is loaded
            let source = match &data.source {
                Some(source) => source,
                None => continue,
            };

            // If it is, add the renderable component
            match source {
                RenderableSource::None => {}
                RenderableSource::Model {
                    model,
                    mesh_group_idx,
                    mesh_idx,
                } => {
                    let model = match assets.get(model) {
                        Some(model) => model,
                        None => continue,
                    };

                    let mg = match model.mesh_groups.get(*mesh_group_idx) {
                        Some(mg) => mg,
                        None => continue,
                    };

                    let mesh_instance = match mg.0.get(*mesh_idx) {
                        Some(mesh) => mesh,
                        None => continue,
                    };

                    commands.entities.remove_component::<RenderableData>(entity);
                    commands.entities.add_component(
                        entity,
                        Renderable {
                            mesh: mesh_instance.mesh.clone(),
                            material: model.materials[mesh_instance.material].clone(),
                            layers: RenderLayer::OPAQUE | RenderLayer::SHADOW_CASTER,
                        },
                    );
                }
            }
        }
    }
}

impl From<ApplyRenderableData> for System {
    fn from(sys: ApplyRenderableData) -> Self {
        SystemBuilder::new(sys)
            .with_handler(ApplyRenderableData::on_pre_render)
            .build()
    }
}
