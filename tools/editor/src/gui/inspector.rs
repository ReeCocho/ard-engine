use std::any::TypeId;
use std::path::PathBuf;

use ard_engine::assets::prelude::{AnyHandle, Asset, AssetName, AssetNameBuf, Assets, Handle};
use ard_engine::ecs::id_map::TypeIdMap;
use ard_engine::ecs::prelude::*;
use ard_engine::graphics_assets::prelude::TextureAsset;
use std::thread::JoinHandle;

use super::asset_meta::AssetMeta;

pub struct Inspector {
    /// Current item being inspected.
    item: Option<ActiveInspectorItem>,
}

/// Event that signals a new item was selected for inspection.
#[derive(Clone, Event)]
pub enum InspectorItem {
    Asset(AssetNameBuf),
}

enum ActiveInspectorItem {
    Asset(AssetInspectorItem),
}

enum AssetInspectorItem {
    /// The asset did not have an associated meta file, and it needs to be created.
    CreatingMeta {
        asset: AssetNameBuf,
        asset_meta: AssetNameBuf,
        wait_for_load: Option<JoinHandle<()>>,
    },
    /// The asset has a meta file and it is being loaded.
    LoadingMeta {
        asset: AssetNameBuf,
        handle: Handle<AssetMeta>,
        wait_for_load: Option<JoinHandle<()>>,
    },
    Loaded {
        asset: AssetNameBuf,
        handle: Handle<AssetMeta>,
    },
    /// There was an error loading/creating the meta file.
    Error,
}

impl Inspector {
    pub fn new() -> Self {
        Self { item: None }
    }

    pub fn set_inspected_item(&mut self, assets: &Assets, item: Option<InspectorItem>) {
        match item {
            Some(item) => match item {
                InspectorItem::Asset(asset) => {
                    let meta_name = AssetMeta::make_meta_name(&asset);

                    // Check if the meta file exists for the asset
                    let mut path_to_meta = PathBuf::from("./assets/game/");
                    path_to_meta.push(&meta_name);

                    // The meta file exists. We must load it.
                    if path_to_meta.exists() {
                        let handle = assets.load::<AssetMeta>(&meta_name);

                        let assets_cl = assets.clone();
                        let handle_cl = handle.clone();

                        self.item = Some(ActiveInspectorItem::Asset(
                            AssetInspectorItem::LoadingMeta {
                                asset,
                                handle,
                                wait_for_load: Some(std::thread::spawn(move || {
                                    assets_cl.wait_for_load(&handle_cl);
                                })),
                            },
                        ));
                    }
                    // Meta file doesn't exist. We must load the actual asset and generate it
                    else {
                        let assets_cl = assets.clone();
                        let asset_cl = asset.clone();

                        self.item = Some(ActiveInspectorItem::Asset(
                            AssetInspectorItem::CreatingMeta {
                                asset,
                                asset_meta: meta_name,
                                wait_for_load: Some(std::thread::spawn(move || {
                                    AssetMeta::initialize_for(assets_cl, asset_cl)
                                })),
                            },
                        ));
                    }
                }
            },
            None => self.item = None,
        }
    }

    pub fn draw(&mut self, ui: &imgui::Ui, assets: &mut Assets) {
        let style = unsafe { ui.style() };

        ui.window("Inspector").build(|| {
            let mut item = match &mut self.item {
                Some(item) => item,
                None => return,
            };

            match item {
                ActiveInspectorItem::Asset(asset_item) => match asset_item {
                    AssetInspectorItem::CreatingMeta { asset, asset_meta, wait_for_load } => {
                        // Check if we've finished loading the meta asset
                        let finished = match wait_for_load {
                            Some(waiting) => waiting.is_finished(),
                            None => {
                                *asset_item = AssetInspectorItem::Error;
                                return
                            }
                        };

                        if !finished {
                            ui.text("Loading...");
                            ui.same_line();
                            throbber(
                                ui,
                                8.0,
                                4.0,
                                8,
                                1.0,
                                style[imgui::StyleColor::Button]
                            );
                            return;
                        }

                        // Check if the load was successful
                        match wait_for_load.take().unwrap().join() {
                            Ok(_) => {
                                *asset_item = AssetInspectorItem::Loaded {
                                    asset: asset.clone(),
                                    handle: assets.get_handle::<AssetMeta>(asset_meta).unwrap()
                                };
                            },
                            Err(_) => {
                                *asset_item = AssetInspectorItem::Error;
                            },
                        }
                    },
                    AssetInspectorItem::LoadingMeta { asset, handle, wait_for_load } => {
                        // Check if we've finished loading the meta asset
                        let finished = match wait_for_load {
                            Some(waiting) => waiting.is_finished(),
                            None => {
                                *asset_item = AssetInspectorItem::Error;
                                return
                            }
                        };

                        if !finished {
                            ui.text("Loading...");
                            ui.same_line();
                            throbber(
                                ui,
                                8.0,
                                4.0,
                                8,
                                1.0,
                                style[imgui::StyleColor::Button]
                            );
                            return;
                        }

                        // Check if the load was successful
                        match wait_for_load.take().unwrap().join() {
                            Ok(_) => {
                                *asset_item = AssetInspectorItem::Loaded {
                                    asset: asset.clone(),
                                    handle: handle.clone()
                                };
                            },
                            Err(_) => {
                                *asset_item = AssetInspectorItem::Error;
                            },
                        }
                    },
                    AssetInspectorItem::Loaded { asset, handle } => {
                        // Draw the header
                        ui.text(asset.file_stem().unwrap_or_default().to_str().unwrap_or_default());
                        ui.separator();

                        // Draw the asset inspector
                        assets.get_mut(handle).unwrap().draw(ui);
                    },
                    AssetInspectorItem::Error => {
                        ui.text("There was an error when trying to load the asset. Please check the logs.");
                    },
                },
            }
        });
    }
}

fn throbber(
    ui: &imgui::Ui,
    radius: f32,
    thickness: f32,
    num_segments: i32,
    speed: f32,
    color: impl Into<imgui::ImColor32>,
) {
    let mut pos = ui.cursor_pos();
    let wpos = ui.window_pos();
    pos[0] += wpos[0];
    pos[1] += wpos[1];

    let size = [radius * 2.0, radius * 2.0];

    let rect = imgui::sys::ImRect {
        Min: imgui::sys::ImVec2::new(pos[0] - thickness, pos[1] - thickness),
        Max: imgui::sys::ImVec2::new(pos[0] + size[0] + thickness, pos[1] + size[1] + thickness),
    };

    unsafe {
        imgui::sys::igItemSizeRect(rect, 0.0);

        if !imgui::sys::igItemAdd(
            rect,
            0,
            std::ptr::null(),
            imgui::sys::ImGuiItemFlags_None as i32,
        ) {
            return;
        }
    }

    let time = ui.time() as f32 * speed;

    let start = (time.sin() * (num_segments - 5) as f32).abs() as i32;
    let min = 2.0 * std::f32::consts::PI * (start as f32 / num_segments as f32);
    let max = 2.0 * std::f32::consts::PI * ((num_segments - 3) as f32 / num_segments as f32);
    let center = [pos[0] + radius, pos[1] + radius];

    let mut points = Vec::with_capacity(num_segments as usize);

    for i in 0..num_segments {
        let a = min + (i as f32 / num_segments as f32) * (max - min);
        let x = (a + time * 8.0).cos() * radius;
        let y = (a + time * 8.0).sin() * radius;
        let new_pos = [center[0] + x, center[1] + y];

        points.push(new_pos);
    }

    // NOTE: Polyline is supposed to be in window coordinates, but for whatever reason it is
    // actually in screen coordinates here. If the throbber ever bugs out, check this first.
    ui.get_window_draw_list()
        .add_polyline(points, color.into())
        .thickness(thickness)
        .build();
}
