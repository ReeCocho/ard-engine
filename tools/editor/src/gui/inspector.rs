use std::path::PathBuf;

use ard_engine::assets::prelude::{AssetNameBuf, Assets, Handle};
use ard_engine::ecs::prelude::*;
use ard_engine::game::object::empty::EmptyObject;
use ard_engine::game::object::static_object::StaticObject;
use ard_engine::game::SceneGameObject;
use ard_engine::graphics::prelude::Factory;

use crate::asset_meta::{AssetMeta, AssetMetaError};
use crate::inspect::Inspect;
use crate::par_task::ParTask;
use crate::scene_graph::SceneGraph;

use super::dirty_assets::DirtyAssets;

pub struct Inspector {
    /// Current item being inspected.
    item: Option<ActiveInspectorItem>,
}

/// Event that signals a new item was selected for inspection.
#[derive(Clone, Event)]
pub enum InspectorItem {
    Asset(AssetNameBuf),
    Entity(Entity),
}

enum ActiveInspectorItem {
    Asset {
        display_name: String,
        asset_name: AssetNameBuf,
        task: ParTask<Handle<AssetMeta>, AssetMetaError>,
    },
    Entity {
        entity: Entity,
        ty: SceneGameObject,
    },
}

impl Inspector {
    pub fn new() -> Self {
        Self { item: None }
    }

    pub fn set_inspected_item(
        &mut self,
        assets: &Assets,
        scene_graph: &SceneGraph,
        item: Option<InspectorItem>,
    ) {
        match item {
            Some(item) => match item {
                InspectorItem::Asset(asset) => {
                    let meta_name = AssetMeta::make_meta_name(&asset);
                    let display_name: String = asset
                        .file_stem()
                        .unwrap_or_default()
                        .to_str()
                        .unwrap_or_default()
                        .into();

                    // Check if the meta file exists for the asset
                    let mut path_to_meta = PathBuf::from("./assets/game/");
                    path_to_meta.push(&meta_name);

                    // The meta file exists. We must load it.
                    if path_to_meta.exists() {
                        let handle = assets.load::<AssetMeta>(&meta_name);

                        let assets_cl = assets.clone();
                        let handle_cl = handle.clone();

                        self.item = Some(ActiveInspectorItem::Asset {
                            display_name,
                            asset_name: asset.clone(),
                            task: ParTask::new(move || {
                                assets_cl.wait_for_load(&handle_cl);
                                Ok(handle_cl)
                            }),
                        });
                    }
                    // Meta file doesn't exist. We must load the actual asset and generate it
                    else {
                        let assets_cl = assets.clone();
                        let asset_cl = asset.clone();

                        self.item = Some(ActiveInspectorItem::Asset {
                            display_name,
                            asset_name: asset.clone(),
                            task: ParTask::new(move || {
                                AssetMeta::initialize_for(assets_cl, asset_cl)
                            }),
                        });
                    }
                }
                InspectorItem::Entity(entity) => {
                    self.item =
                        scene_graph
                            .find_entity(entity)
                            .map(|node| ActiveInspectorItem::Entity {
                                entity,
                                ty: node.ty,
                            });
                }
            },
            None => self.item = None,
        }
    }

    pub fn draw(
        &mut self,
        ui: &imgui::Ui,
        commands: &Commands,
        queries: &Queries<Everything>,
        assets: &mut Assets,
        dirty: &mut DirtyAssets,
        factory: &Factory,
        scene_graph: &SceneGraph,
    ) {
        ui.window("Inspector").build(|| {
            let item = match &mut self.item {
                Some(item) => item,
                None => return,
            };

            match item {
                ActiveInspectorItem::Asset {
                    display_name,
                    asset_name,
                    task,
                } => {
                    task.ui(ui, |handle| {
                        // Draw the header
                        ui.text(display_name);
                        ui.separator();

                        // Draw the asset inspector
                        let modified = match assets.get_mut(handle) {
                            Some(mut asset) => asset.draw(ui, assets, factory),
                            None => {
                                ui.text("There was an error loading the asset. Check the logs.");
                                false
                            }
                        };

                        if modified {
                            dirty.add(asset_name, handle.clone());
                        }
                    });
                }
                ActiveInspectorItem::Entity { entity, ty } => match ty {
                    SceneGameObject::StaticObject => {
                        StaticObject::inspect(ui, *entity, commands, queries, assets)
                    }
                    SceneGameObject::EmptyObject => {
                        EmptyObject::inspect(ui, *entity, commands, queries, assets)
                    }
                },
            }
        });
    }
}
