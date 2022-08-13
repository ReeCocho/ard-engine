pub mod inspect;

use std::path::PathBuf;

use inspect::Inspect;

use ard_engine::{
    assets::prelude::*,
    ecs::prelude::*,
    game::{
        object::{empty::EmptyObject, static_object::StaticObject},
        SceneGameObject,
    },
    graphics::prelude::*,
    math::*,
};

use crate::util::{
    asset_meta::{AssetMeta, AssetMetaError},
    par_task::ParTask,
};

use crate::scene_graph::SceneGraph;

use super::{scene_view::SceneViewCamera, View};

const GIZMO_SCALE_FACTOR: f32 = 0.25;

pub struct Inspector {
    transform_gizmo: TransformGizmo,
    /// Current item being inspected.
    item: Option<ActiveInspectorItem>,
}

struct TransformGizmo {}

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
        Self {
            item: None,
            transform_gizmo: TransformGizmo {},
        }
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
}

impl View for Inspector {
    fn show(
        &mut self,
        ui: &imgui::Ui,
        _controller: &mut crate::controller::Controller,
        resc: &mut crate::editor::Resources,
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
                        let modified = match resc.assets.get_mut(handle) {
                            Some(mut asset) => asset.draw(ui, resc.assets, resc.factory),
                            None => {
                                ui.text("There was an error loading the asset. Check the logs.");
                                false
                            }
                        };

                        if modified {
                            resc.dirty.add(asset_name, handle.clone());
                        }
                    });
                }
                ActiveInspectorItem::Entity { entity, ty } => {
                    // Draw the bounding box of the object unless it has no mesh to render
                    if let Some(query) = resc
                        .queries
                        .get::<(Read<Renderable<VkBackend>>, Read<Model>)>(*entity)
                    {
                        let bounds = query.0.mesh.bounds();
                        let model = query.1 .0 * Mat4::from_translation(bounds.center.xyz());

                        // Draw object bounds
                        resc.debug_draw.draw_rect_prism(
                            bounds.half_extents.xyz(),
                            model,
                            Vec3::new(1.0, 1.0, 0.0),
                        );

                        // Draw the transform gizmo
                        self.transform_gizmo
                            .draw(resc.debug_draw, resc.camera, query.1 .0);
                    }

                    // Inspect the object
                    match ty {
                        SceneGameObject::StaticObject => {
                            StaticObject::inspect(
                                ui,
                                *entity,
                                &resc.ecs_commands,
                                &resc.queries,
                                &resc.assets,
                            );
                        }
                        SceneGameObject::EmptyObject => {
                            EmptyObject::inspect(
                                ui,
                                *entity,
                                &resc.ecs_commands,
                                &resc.queries,
                                &resc.assets,
                            );
                        }
                    }
                }
            }
        });
    }
}

impl TransformGizmo {
    pub fn draw(&self, drawing: &DebugDrawing, view: &SceneViewCamera, model: Mat4) {
        let pos = model.col(3).xyz();

        let scale = Vec3::new(
            model.col(0).xyz().length() * model.col(0).x.signum(),
            model.col(1).xyz().length() * model.col(1).y.signum(),
            model.col(2).xyz().length() * model.col(2).z.signum(),
        );

        let rot = Mat4::from_cols(
            if scale.x == 0.0 {
                Vec4::X
            } else {
                Vec4::from((model.col(0).xyz() / scale.x, 0.0))
            },
            if scale.y == 0.0 {
                Vec4::Y
            } else {
                Vec4::from((model.col(1).xyz() / scale.y, 0.0))
            },
            if scale.z == 0.0 {
                Vec4::Z
            } else {
                Vec4::from((model.col(2).xyz() / scale.z, 0.0))
            },
            Vec4::new(0.0, 0.0, 0.0, 1.0),
        );

        let model = Mat4::from_translation(pos)
            * rot
            * Mat4::from_scale(Vec3::ONE * (pos - view.position).length() * GIZMO_SCALE_FACTOR);

        let origin = (model * Vec4::new(0.0, 0.0, 0.0, 1.0)).xyz();
        let x = (model * Vec4::from((Vec3::X, 1.0))).xyz();
        let y = (model * Vec4::from((Vec3::Y, 1.0))).xyz();
        let z = (model * Vec4::from((Vec3::Z, 1.0))).xyz();

        // X-axis
        drawing.draw_line(origin, x, Vec3::X);
        draw_translate_tip(
            drawing,
            Vec3::X,
            Vec3::new(0.0, 0.0, -std::f32::consts::FRAC_PI_2),
            model,
        );

        // Y-axis
        drawing.draw_line(origin, y, Vec3::Y);
        draw_translate_tip(drawing, Vec3::Y, Vec3::ZERO, model);

        // Z-axis
        drawing.draw_line(origin, z, Vec3::Z);
        draw_translate_tip(
            drawing,
            Vec3::Z,
            Vec3::new(std::f32::consts::FRAC_PI_2, 0.0, 0.0),
            model,
        );
    }
}

fn draw_translate_tip(drawing: &DebugDrawing, color: Vec3, rotation: Vec3, model: Mat4) {
    const TIP_SIZE: f32 = 0.07;

    // Three lines for the triangle base and three more for the tip
    let mut points = [(Vec3::ZERO, Vec3::ZERO); 6];

    // Base
    for i in 0..3 {
        let ang1 = (i as f32 * 120.0) * std::f32::consts::PI / 180.0;
        let ang2 = ((i + 1) as f32 * 120.0) * std::f32::consts::PI / 180.0;

        points[i].0 = Vec3::new(ang1.cos(), 0.0, ang1.sin()) * TIP_SIZE;

        points[i].1 = Vec3::new(ang2.cos(), 0.0, ang2.sin()) * TIP_SIZE;
    }

    // Tip
    for i in 3..6 {
        let ang1 = (i as f32 * 120.0) * std::f32::consts::PI / 180.0;

        points[i].0 = Vec3::new(ang1.cos(), 0.0, ang1.sin()) * TIP_SIZE;

        points[i].1 = Vec3::new(0.0, 3.0, 0.0) * TIP_SIZE;
    }

    // Apply model matrix and then draw
    let model = model
        * Mat4::from_euler(EulerRot::XYZ, rotation.x, rotation.y, rotation.z)
        * Mat4::from_translation(Vec3::Y);
    for pt in &mut points {
        pt.0 = (model * Vec4::from((pt.0, 1.0))).xyz();
        pt.1 = (model * Vec4::from((pt.1, 1.0))).xyz();
        drawing.draw_line(pt.0, pt.1, color);
    }
}
