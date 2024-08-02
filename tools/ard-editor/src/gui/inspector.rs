use std::{
    ops::{Deref, DerefMut},
    path::PathBuf,
    time::Duration,
};

use ard_engine::{
    assets::{asset::AssetNameBuf, handle::Handle, prelude::Assets},
    core::core::Tick,
    ecs::prelude::*,
    formats::material::{BlendType, MaterialType},
    game::components::player::PlayerSpawn,
    math::{Mat4, Vec3, Vec3A, Vec4, Vec4Swizzles},
    physics::{
        collider::{CoefficientCombineRule, Collider},
        rigid_body::RigidBody,
    },
    render::{
        factory::Factory,
        material::MaterialAsset,
        prelude::{Filter, SamplerAddressMode},
        shape::Shape,
        texture::TextureAsset,
        DebugDraw, DebugDrawing, Mesh, PbrMaterialData, TextureSlot, PBR_MATERIAL_DIFFUSE_SLOT,
        PBR_MATERIAL_METALLIC_ROUGHNESS_SLOT, PBR_MATERIAL_NORMAL_SLOT,
    },
    transform::Model,
};
use camino::Utf8PathBuf;
use path_macro::path;
use rustc_hash::FxHashMap;

use crate::{
    assets::{
        meta::{AssetType, MetaData, TextureImportSettings, TextureMipSetting},
        EditorAssets,
    },
    gui::util,
    inspect::{
        collider::ColliderInspector, material::MaterialInspector, player::PlayerSpawnInspector,
        rigid_body::RigidBodyInspector, transform::TransformInspector, Inspectors,
    },
    selected::Selected,
    tasks::{material::SaveMaterialTask, texture::TextureImportTask, TaskQueue},
};

use super::EditorViewContext;

pub struct InspectorView {
    inspectors: Inspectors,
    add_component: FxHashMap<String, AddComponentFn>,
}

#[derive(SystemState)]
pub struct InspectorChangeDetectSystem;

const CHANGE_TIMER_DUR: Duration = Duration::from_millis(300);

#[derive(Resource, Default)]
pub struct Inspected {
    change_timer: Option<Duration>,
    obj: InspectedObject,
}

#[derive(Default)]
pub enum InspectedObject {
    #[default]
    None,
    Material(Handle<MaterialAsset>),
    Texture {
        handle: Handle<TextureAsset>,
        meta: Utf8PathBuf,
    },
    Entity(Entity),
}

type AddComponentFn = Box<dyn Fn(Entity, &Commands, &Queries<Everything>, &Res<Everything>)>;

impl Default for InspectorView {
    fn default() -> Self {
        let mut inspectors = Inspectors::default();
        inspectors.with(TransformInspector);
        inspectors.with(MaterialInspector);
        inspectors.with(ColliderInspector);
        inspectors.with(RigidBodyInspector);
        inspectors.with(PlayerSpawnInspector);

        let mut add_component = FxHashMap::default();
        add_component.insert(
            Collider::NAME.into(),
            add_component_fn(|entity, queries, _| {
                let (shape, offset) = match queries.get::<Read<Mesh>>(entity) {
                    Some(mesh) => {
                        let model = Model(
                            queries
                                .get::<Read<Model>>(entity)
                                .map(|mdl| mdl.0)
                                .unwrap_or(Mat4::IDENTITY),
                        );
                        let scale = Vec3::from(model.scale().abs().max(Vec3A::ONE * 0.05));

                        let bounds = mesh.bounds();
                        let offset = scale * ((bounds.max_pt.xyz() + bounds.min_pt.xyz()) * 0.5);
                        let shape = ard_engine::physics::collider::Shape::Box {
                            half_extents: scale
                                * ((bounds.max_pt.xyz() - bounds.min_pt.xyz()) * 0.5),
                        };

                        (shape, offset)
                    }
                    None => (
                        ard_engine::physics::collider::Shape::Box {
                            half_extents: Vec3::new(2.0, 2.0, 2.0),
                        },
                        Vec3::ZERO,
                    ),
                };

                Collider {
                    shape,
                    offset,
                    friction: 0.8,
                    friction_combine_rule: CoefficientCombineRule::Max,
                    restitution: 0.1,
                    restitution_combine_rule: CoefficientCombineRule::Max,
                    mass: 1.0,
                }
            }),
        );

        add_component.insert(
            RigidBody::NAME.into(),
            add_component_fn(|_, _, _| RigidBody::default()),
        );

        add_component.insert(
            PlayerSpawn::NAME.into(),
            add_component_fn(|_, _, _| PlayerSpawn),
        );

        Self {
            inspectors,
            add_component,
        }
    }
}

impl InspectorView {
    pub fn show(&mut self, ctx: EditorViewContext) -> egui_tiles::UiResponse {
        let mut inspected = ctx.res.get_mut::<Inspected>().unwrap();

        let changed = match &mut inspected.obj {
            InspectedObject::None => false,
            InspectedObject::Entity(e) => {
                if ctx.queries.get::<Read<Model>>(*e).is_none() {
                    return egui_tiles::UiResponse::None;
                }
                self.inspect_entity(ctx, *e)
            }
            InspectedObject::Material(asset) => {
                let assets = ctx.res.get::<Assets>().unwrap();
                let mut mat = match assets.get_mut(asset) {
                    Some(mat) => mat,
                    None => return egui_tiles::UiResponse::None,
                };
                self.inspect_material(ctx, mat.deref_mut())
            }
            InspectedObject::Texture { handle, meta } => {
                let assets = ctx.res.get::<Assets>().unwrap();
                let mut editor_assets = ctx.res.get_mut::<EditorAssets>().unwrap();
                let _tex = match assets.get_mut(handle) {
                    Some(tex) => tex,
                    None => return egui_tiles::UiResponse::None,
                };
                let assets_root = editor_assets.active_assets_root().to_owned();
                let meta = match editor_assets.find_asset_mut(meta) {
                    Some(meta) => meta,
                    None => return egui_tiles::UiResponse::None,
                };
                let path = path!(assets_root / meta.raw_path());
                let baked_path = meta.meta_file().baked.clone();
                let settings = match &mut meta.meta_file_mut().data {
                    MetaData::Texture(settings) => settings,
                    _ => return egui_tiles::UiResponse::None,
                };

                self.inspect_texture(ctx, path, baked_path, settings);
                false
            }
        };

        if changed {
            inspected.mark_change();
        }

        egui_tiles::UiResponse::None
    }

    fn inspect_entity(&mut self, ctx: EditorViewContext, entity: Entity) -> bool {
        if !ctx.queries.is_alive(entity) {
            return true;
        }

        if let Some(query) = ctx.queries.get::<(Read<Model>, Read<Mesh>)>(entity) {
            let (model, mesh) = *query;
            let bounds = mesh.bounds();
            ctx.res.get_mut::<DebugDrawing>().unwrap().draw(DebugDraw {
                //                          \m/
                color: Vec4::new(1.0, 0.666, 0.0, 1.0),
                shape: Shape::Box {
                    min_pt: bounds.min_pt.xyz(),
                    max_pt: bounds.max_pt.xyz(),
                    model: model.0,
                },
            });
        }

        self.inspectors
            .show(ctx.ui, entity, ctx.commands, ctx.queries, ctx.res);

        ctx.ui.menu_button("Add Component", |ui| {
            for (name, func) in self.add_component.iter() {
                if ui.button(name).clicked() {
                    func(entity, ctx.commands, ctx.queries, ctx.res);
                }
            }
        });

        false
    }

    fn inspect_texture(
        &self,
        ctx: EditorViewContext,
        raw_path: PathBuf,
        baked_asset: AssetNameBuf,
        settings: &mut TextureImportSettings,
    ) {
        fn sampler_combo_box(
            ui: &mut egui::Ui,
            id: impl std::hash::Hash,
            value: &mut SamplerAddressMode,
        ) -> egui::Response {
            egui::ComboBox::new(id, "")
                .selected_text(format!("{:?}", *value))
                .show_ui(ui, |ui| {
                    ui.selectable_value(value, SamplerAddressMode::Repeat, "Repeat");
                    ui.selectable_value(
                        value,
                        SamplerAddressMode::MirroredRepeat,
                        "Mirrored Repeat",
                    );
                    ui.selectable_value(value, SamplerAddressMode::ClampToEdge, "Clamp To Edge");
                })
                .response
        }

        fn filter_combo_box(
            ui: &mut egui::Ui,
            id: impl std::hash::Hash,
            value: &mut Filter,
        ) -> egui::Response {
            egui::ComboBox::new(id, "")
                .selected_text(format!("{:?}", *value))
                .show_ui(ui, |ui| {
                    ui.selectable_value(value, Filter::Linear, "Linear");
                    ui.selectable_value(value, Filter::Nearest, "Nearest");
                })
                .response
        }

        ctx.ui.heading("Texture");
        ctx.ui.separator();

        egui::Grid::new("texture_grid")
            .num_columns(2)
            .spacing([30.0, 20.0])
            .min_col_width(ctx.ui.available_width() * 0.5)
            .striped(true)
            .show(ctx.ui, |ui| {
                ui.label("Address Mode (Horizontal)");
                sampler_combo_box(ui, "address_move_h", &mut settings.sampler.address_u);
                ui.end_row();

                ui.label("Address Mode (Vertical)");
                sampler_combo_box(ui, "address_move_v", &mut settings.sampler.address_v);
                ui.end_row();

                ui.label("Min. Filter");
                filter_combo_box(ui, "min_filter", &mut settings.sampler.min_filter);
                ui.end_row();

                ui.label("Mag. Filter");
                filter_combo_box(ui, "mag_filter", &mut settings.sampler.mag_filter);
                ui.end_row();

                ui.label("Mip Map Filter");
                filter_combo_box(ui, "mip_map_filter", &mut settings.sampler.mipmap_filter);
                ui.end_row();

                ui.label("Mip Map Type");
                egui::ComboBox::new("mip_map_type", "")
                    .selected_text(settings.mip.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut settings.mip,
                            TextureMipSetting::GenerateAll,
                            "Generate All",
                        );
                        ui.selectable_value(
                            &mut settings.mip,
                            TextureMipSetting::GenerateExact(1),
                            "Generate Exact",
                        );
                        ui.selectable_value(&mut settings.mip, TextureMipSetting::None, "None");
                    });
                ui.end_row();

                if let TextureMipSetting::GenerateExact(count) = &mut settings.mip {
                    ui.label("Mip Count");
                    ui.add(egui::DragValue::new(count).range(1..=16));
                    ui.end_row();
                }

                ui.label("Anisotropic Filtering");
                ui.checkbox(&mut settings.sampler.anisotropy, "")
                    .on_hover_ui(|ui| {
                        ui.label("Improves image quality when viewed at shearing angles.");
                        ui.hyperlink("https://en.wikipedia.org/wiki/Anisotropic_filtering");
                    });
                ui.end_row();

                ui.label("Compress").on_hover_text(
                    "Enables texture compression. This should be enabled unless the precise \
                    values of the texture are important.",
                );
                ui.checkbox(&mut settings.compress, "");
                ui.end_row();

                ui.label("Linear Color Space").on_hover_text(
                    "Enable when the texture contains non-color data (normal maps, \
                    metallic/roughness maps, etc.)",
                );
                ui.checkbox(&mut settings.linear_color_space, "");
                ui.end_row();
            });

        ctx.ui.separator();
        if ctx.ui.add(util::transformation_button("Apply")).clicked() {
            ctx.res
                .get::<TaskQueue>()
                .unwrap()
                .add(TextureImportTask::reimport(
                    raw_path,
                    baked_asset,
                    *settings,
                ));
        }
    }

    fn inspect_material(&self, ctx: EditorViewContext, material: &mut MaterialAsset) -> bool {
        let factory = ctx.res.get::<Factory>().unwrap();
        let assets = ctx.res.get::<Assets>().unwrap();

        fn texture_input(
            ui: &mut egui::Ui,
            assets: &Assets,
            factory: &Factory,
            asset: &mut MaterialAsset,
            slot: TextureSlot,
            map: &mut Option<Utf8PathBuf>,
        ) -> bool {
            util::drag_drop_asset_target(
                ui,
                map.clone().map(|n| n.to_string()).unwrap_or_default(),
                |asset| match AssetType::try_from(asset.meta_file().baked.as_std_path()) {
                    Ok(AssetType::Texture) => true,
                    _ => false,
                },
                |tex_asset| match tex_asset {
                    Some(tex_asset) => {
                        let handle = match assets.load::<TextureAsset>(&tex_asset.meta_file().baked)
                        {
                            Some(handle) => handle,
                            None => return false,
                        };

                        asset.set_texture(factory, slot, Some(handle));
                        *map = Some(tex_asset.meta_file().baked.clone());
                        true
                    }
                    None => {
                        *map = None;
                        asset.set_texture(factory, slot, None);
                        true
                    }
                },
            )
        }

        ctx.ui.heading("Material");
        ctx.ui.separator();

        let mut changed = false;
        let mut render_mode_changed = false;

        egui::Grid::new("material_grid")
            .num_columns(2)
            .spacing([30.0, 20.0])
            .striped(true)
            .show(ctx.ui, |ui| {
                ui.label("Rendering Mode");
                render_mode_changed = egui::ComboBox::new("material_rendering_mode", "")
                    .selected_text(format!("{:?}", material.header.blend_ty))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut material.header.blend_ty,
                            BlendType::Opaque,
                            "Opaque",
                        );
                        ui.selectable_value(
                            &mut material.header.blend_ty,
                            BlendType::Blend,
                            "Transparent",
                        );
                        ui.selectable_value(
                            &mut material.header.blend_ty,
                            BlendType::Mask,
                            "Alpha Cutout",
                        );
                    })
                    .response
                    .changed();
                ui.end_row();

                let mut header = material.header.ty.clone();
                match &mut header {
                    MaterialType::Pbr {
                        base_color,
                        metallic,
                        roughness,
                        alpha_cutoff,
                        diffuse_map,
                        normal_map,
                        metallic_roughness_map,
                    } => {
                        ui.label("Base Color");
                        let mut color = base_color.to_array();
                        changed |= ui
                            .color_edit_button_rgba_premultiplied(&mut color)
                            .changed();
                        *base_color = color.into();
                        ui.end_row();

                        ui.label("Metallic");
                        changed |= ui.add(egui::Slider::new(metallic, 0.0..=1.0)).changed();
                        ui.end_row();

                        ui.label("Roughness");
                        changed |= ui.add(egui::Slider::new(roughness, 0.0..=1.0)).changed();
                        ui.end_row();

                        ui.label("Alpha Cutoff");
                        changed |= ui.add(egui::Slider::new(alpha_cutoff, 0.0..=1.0)).changed();
                        ui.end_row();

                        ui.label("Diffuse Map");
                        changed |= texture_input(
                            ui,
                            &assets,
                            &factory,
                            material,
                            TextureSlot::from(PBR_MATERIAL_DIFFUSE_SLOT),
                            diffuse_map,
                        );
                        ui.end_row();

                        ui.label("Normal Map");
                        changed |= texture_input(
                            ui,
                            &assets,
                            &factory,
                            material,
                            TextureSlot::from(PBR_MATERIAL_NORMAL_SLOT),
                            normal_map,
                        );
                        ui.end_row();

                        ui.label("Metallic/Roughness Map");
                        changed |= texture_input(
                            ui,
                            &assets,
                            &factory,
                            material,
                            TextureSlot::from(PBR_MATERIAL_METALLIC_ROUGHNESS_SLOT),
                            metallic_roughness_map,
                        );
                        ui.end_row();

                        if changed {
                            factory.set_material_data(
                                &material.instance,
                                &PbrMaterialData {
                                    alpha_cutoff: *alpha_cutoff,
                                    color: *base_color,
                                    metallic: *metallic,
                                    roughness: *roughness,
                                },
                            );
                        }
                    }
                }

                if changed {
                    material.header.ty = header;
                }
            });

        changed || render_mode_changed
    }
}

impl Inspected {
    pub fn mark_change(&mut self) {
        self.change_timer = Some(CHANGE_TIMER_DUR);
    }
}

impl InspectorChangeDetectSystem {
    fn tick(
        &mut self,
        tick: Tick,
        _: Commands,
        _: Queries<()>,
        res: Res<(
            Write<Inspected>,
            Read<Selected>,
            Read<Assets>,
            Read<EditorAssets>,
            Read<Assets>,
            Read<TaskQueue>,
        )>,
    ) {
        let mut inspected = res.get_mut::<Inspected>().unwrap();
        let inspected = inspected.deref_mut();

        let selected = res.get::<Selected>().unwrap();
        let selected = selected.deref();

        let task_queue = res.get::<TaskQueue>().unwrap();
        let editor_assets = res.get::<EditorAssets>().unwrap();
        let assets = res.get::<Assets>().unwrap();

        // Update change timer
        let mut should_save = match &mut inspected.change_timer {
            Some(timer) => {
                *timer = timer.saturating_sub(tick.0);
                *timer == Duration::ZERO
            }
            None => false,
        };

        if should_save {
            inspected.change_timer = None;
        }

        // Check if we have a mismatch between the selected object and inspected object
        let mismatch = match (selected, &mut inspected.obj) {
            // Both none. No mismatch
            (Selected::None, InspectedObject::None) => false,
            // Need to save if the entities mismatch
            (Selected::Entity(selected), InspectedObject::Entity(inspected)) => {
                *selected != *inspected
            }
            // Need to save if the asset mismatches
            (Selected::Asset(asset), InspectedObject::Material(mat)) => {
                match editor_assets.find_asset(asset) {
                    Some(asset) => asset.meta_file().baked != assets.get_name(mat),
                    None => true,
                }
            }
            (Selected::Asset(asset), InspectedObject::Texture { handle, .. }) => {
                match editor_assets.find_asset(asset) {
                    Some(asset) => asset.meta_file().baked != assets.get_name(handle),
                    None => true,
                }
            }
            _ => true,
        };

        should_save |= mismatch;

        if should_save {
            match &inspected.obj {
                InspectedObject::None => {}
                // TODO: Save changes from components for undo/redo
                InspectedObject::Entity(_) => {}
                InspectedObject::Material(mat) => {
                    task_queue.add(SaveMaterialTask::new(mat.clone()));
                }
                InspectedObject::Texture { .. } => {}
            }
        }

        if mismatch {
            inspected.obj = match selected {
                Selected::None => InspectedObject::None,
                Selected::Entity(entity) => InspectedObject::Entity(*entity),
                Selected::Asset(asset) => 'asset: {
                    let asset = match editor_assets.find_asset(asset) {
                        Some(asset) => asset,
                        None => break 'asset InspectedObject::None,
                    };

                    let ty = match AssetType::try_from(asset.meta_file().baked.as_std_path()) {
                        Ok(ty) => ty,
                        Err(_) => break 'asset InspectedObject::None,
                    };

                    match ty {
                        AssetType::Material => {
                            match assets.load::<MaterialAsset>(&asset.meta_file().baked) {
                                Some(handle) => InspectedObject::Material(handle),
                                None => InspectedObject::None,
                            }
                        }
                        AssetType::Texture => {
                            match assets.load::<TextureAsset>(&asset.meta_file().baked) {
                                Some(handle) => InspectedObject::Texture {
                                    handle,
                                    meta: asset.meta_path().into(),
                                },
                                None => InspectedObject::None,
                            }
                        }
                        _ => InspectedObject::None,
                    }
                }
            };
            inspected.change_timer = None;
        }
    }
}

fn add_component_fn<C: Component + 'static>(
    func: impl Fn(Entity, &Queries<Everything>, &Res<Everything>) -> C + 'static,
) -> AddComponentFn {
    Box::new(move |entity, commands, queries, res| {
        let component = func(entity, queries, res);
        commands.entities.add_component(entity, component);
    })
}

impl From<InspectorChangeDetectSystem> for System {
    fn from(value: InspectorChangeDetectSystem) -> Self {
        SystemBuilder::new(value)
            .with_handler(InspectorChangeDetectSystem::tick)
            .build()
    }
}
