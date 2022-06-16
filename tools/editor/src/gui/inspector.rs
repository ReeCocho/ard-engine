use core::num;
use std::any::TypeId;
use std::collections::HashMap;

use ard_engine::assets::prelude::{AnyHandle, Asset, AssetName, AssetNameBuf, Assets};
use ard_engine::ecs::id_map::TypeIdMap;
use ard_engine::ecs::prelude::*;
use ard_engine::graphics::prelude::Texture;
use ard_engine::graphics_assets::prelude::TextureAsset;
use futures::FutureExt;
use std::thread::JoinHandle;

pub struct Inspector {
    /// Current item being inspected.
    item: Option<ActiveInspectorItem>,
    /// Maps the type ids of assets to the function that draws their inspector.
    displays:
        TypeIdMap<fn(&imgui::Ui, &AssetName, &mut Assets) -> (Option<JoinHandle<()>>, AnyHandle)>,
}

/// Event that signals a new item was selected for inspection.
#[derive(Clone, Event)]
pub enum InspectorItem {
    Asset(AssetNameBuf),
}

enum ActiveInspectorItem {
    Asset {
        // The actual asset to inspect
        asset: AssetNameBuf,
        /// Handle to the asset.
        handle: Option<AnyHandle>,
        // An error has occured during load of an asset.
        error_loading: bool,
        // An asynconronous task that indicates when the asset is done loading.
        wait_for_load: Option<JoinHandle<()>>,
    },
}

impl Inspector {
    pub fn new() -> Self {
        let mut displays = TypeIdMap::<
            fn(&imgui::Ui, &AssetName, &mut Assets) -> (Option<JoinHandle<()>>, AnyHandle),
        >::default();
        displays.insert(
            TypeId::of::<TextureAsset>(),
            draw_asset_inspector::<TextureAsset>,
        );

        Self {
            item: None,
            displays,
        }
    }

    pub fn set_inspected_item(&mut self, item: Option<InspectorItem>) {
        match item {
            Some(item) => match item {
                InspectorItem::Asset(asset) => {
                    self.item = Some(ActiveInspectorItem::Asset {
                        asset,
                        error_loading: false,
                        handle: None,
                        wait_for_load: None,
                    })
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
                ActiveInspectorItem::Asset {
                    asset,
                    error_loading,
                    handle,
                    wait_for_load
                } => {
                    // If the asset doesn't exist, we deselect it and stop displaying
                    if !assets.exists(asset) {
                        self.item = None;
                        return;
                    }

                    // Draw the header
                    ui.text(asset.file_stem().unwrap_or_default().to_str().unwrap_or_default());
                    ui.separator();

                    // If there is an error loading, notify
                    if *error_loading {
                        ui.text("There was an error when trying to load the asset. Please check the logs.");
                        return;
                    }

                    // If we are waiting for load, check if it's complete
                    if let Some(waiting) = wait_for_load {
                        ui.text("Loading...");
                        ui.same_line();

                        throbber(
                            ui,
                            8.0,
                            4.0,
                            8,
                            2.0,
                            style[imgui::StyleColor::Button]
                        );

                        // Check if we are finished waiting
                        if waiting.is_finished() {
                            *wait_for_load = None;

                            // If the asset is still not loaded, there must have been an error
                            if !assets.loaded(asset) {
                                *error_loading = true;
                                return;
                            }
                        }
                        else {
                            return;
                        }
                    }

                    // Determine what asset type we're dealing with
                    match assets.ty_id(asset) {
                        Some(id) => match self.displays.get(&id) {
                            Some(display) => {
                                let (needs_wait, new_handle) = display(ui, asset, assets);
                                if let Some(needs_wait) = needs_wait {
                                    *wait_for_load = Some(needs_wait);
                                }
                                *handle = Some(new_handle);
                            },
                            None => ui.text("Unknown asset type."),
                        },
                        None => {
                            ui.text("Unknown asset type.");
                        },
                    }
                },
            }
        });
    }
}

fn draw_asset_inspector<A: Asset + 'static>(
    ui: &imgui::Ui,
    name: &AssetName,
    assets: &mut Assets,
) -> (Option<JoinHandle<()>>, AnyHandle) {
    // Load the asset
    let handle = assets.load::<A>(name);

    // If we can't get the asset now, we will wait for load
    let join = match assets.get_mut(&handle) {
        Some(mut asset) => {
            asset.gui(ui, assets);
            None
        }
        None => {
            let assets_cl = assets.clone();
            let handle_cl = handle.clone();
            Some(std::thread::spawn(move || {
                assets_cl.wait_for_load(&handle_cl);
            }))
        }
    };

    (join, handle.into())
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
