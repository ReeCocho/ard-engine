use std::path::PathBuf;

use ard_engine::{
    assets::prelude::*, ecs::prelude::*, game::SceneGameObject, graphics::prelude::*, math::*,
};

use crate::{
    inspectable::InspectState,
    util::{
        asset_meta::{AssetMeta, AssetMetaError},
        par_task::ParTask,
    },
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
        controller: &mut crate::controller::Controller,
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
                    // Deselect the entity if it isn't in the scene graph
                    if !resc.scene_graph.contains(*entity) {
                        self.item = None;
                        return;
                    }

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
                        let (x, y) = resc.input.mouse_pos();
                        let mouse_pos = Vec2::new(x as f32, y as f32);
                        self.transform_gizmo.draw(
                            resc.debug_draw,
                            mouse_pos,
                            resc.camera,
                            query.1 .0,
                        );
                    }

                    // Create inspection state
                    let mut inspect_state = InspectState::new_inspection(resc, *entity, *ty, ui);

                    // Inspect the object
                    inspect_state.inspect();

                    // Drain the modify queue and submit to controller
                    for command in inspect_state.into_modify_queue().unwrap().drain() {
                        controller.submit(command);
                    }
                }
            }
        });
    }
}

impl TransformGizmo {
    pub fn draw(
        &self,
        drawing: &DebugDrawing,
        mouse_pos: Vec2,
        view: &SceneViewCamera,
        model: Mat4,
    ) {
        // World space direction from the camera to where the screen is pointing
        let stw = view.screen_to_world(mouse_pos);
        let mouse_dir = (stw - view.position).xyz().normalize();

        // Compute the model matrix of the gizmo
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

        let scale_factor = (pos - view.position).length() * GIZMO_SCALE_FACTOR;
        let translate = Mat4::from_translation(pos);
        let scale = Mat4::from_scale(Vec3::ONE * scale_factor);
        let model = translate * rot * scale;

        let origin = (model * Vec4::new(0.0, 0.0, 0.0, 1.0)).xyz();
        let x = (model * Vec4::from((Vec3::X, 1.0))).xyz();
        let y = (model * Vec4::from((Vec3::Y, 1.0))).xyz();
        let z = (model * Vec4::from((Vec3::Z, 1.0))).xyz();

        // Rotate the mouse direction vector to be relative to the gizmo
        let forward = (rot.inverse() * Vec4::from((mouse_dir, 1.0))).xyz();
        let position = ((translate * rot).inverse() * Vec4::from((view.position, 1.0))).xyz();

        // X-axis
        let bounds = ObjectBounds {
            center: Vec4::new(0.6 * scale_factor, 0.0, 0.0, 0.0),
            half_extents: Vec4::new(0.6, 0.08, 0.08, 1.0) * scale_factor,
        };

        let color = if bounds.intersects(position, forward) {
            Vec3::new(1.0, 0.5, 0.5)
        } else {
            Vec3::X
        };

        drawing.draw_line(origin, x, color);
        draw_translate_tip(
            drawing,
            color,
            Vec3::new(0.0, 0.0, -std::f32::consts::FRAC_PI_2),
            model,
        );

        // Y-axis
        let bounds = ObjectBounds {
            center: Vec4::new(0.0, 0.6 * scale_factor, 0.0, 0.0),
            half_extents: Vec4::new(0.08, 0.6, 0.08, 1.0) * scale_factor,
        };

        let color = if bounds.intersects(position, forward) {
            Vec3::new(0.7, 1.0, 0.7)
        } else {
            Vec3::Y
        };

        drawing.draw_line(origin, y, color);
        draw_translate_tip(drawing, color, Vec3::ZERO, model);

        // Z-axis
        let bounds = ObjectBounds {
            center: Vec4::new(0.0, 0.0, 0.6 * scale_factor, 0.0),
            half_extents: Vec4::new(0.08, 0.08, 0.6, 1.0) * scale_factor,
        };

        let color = if bounds.intersects(position, forward) {
            Vec3::new(0.5, 0.5, 1.0)
        } else {
            Vec3::Z
        };

        drawing.draw_line(origin, z, color);
        draw_translate_tip(
            drawing,
            color,
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
