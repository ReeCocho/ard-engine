use ard_engine::log::*;
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};
use thiserror::*;

use ard_engine::{assets::prelude::*, graphics::prelude::*, graphics_assets::prelude::*, math::*};

use crate::util::{
    par_task::{ParTask, ParTaskGet},
    ui::DragDropPayload,
};

use super::{inspector::InspectorItem, View};

const MAX_CHARS_IN_ASSET: usize = 12;
const ASSET_ICON_SIZE: f32 = 80.0;
const EDITOR_ASSET_FOLDER_NAME: &'static str = "EDITOR_ASSETS";
const FALLBACK_FOLDER_NAME: &'static str = "fallback";

pub struct AssetViewer {
    assets: Assets,
    folder_icon: Handle<TextureAsset>,
    file_icon: Handle<TextureAsset>,
    /// Root folder in the asset view.
    root: Folder,
    /// List of folder names in order that points to the current folder. The root is implied to be
    /// the first element.
    current: Vec<String>,
    /// Task for loading an asset/folder.
    loading: Option<ParTask<PathBuf, AssetLoadError>>,
}

#[derive(Default)]
struct Folder {
    name: String,
    display_name: String,
    folders: Vec<Folder>,
    assets: Vec<Asset>,
}

struct Asset {
    display_name: String,
    asset_name: AssetNameBuf,
}

#[derive(Debug, Error)]
enum AssetLoadError {
    #[error("{0}")]
    Error(String),
}

impl AssetViewer {
    pub fn new(assets: &Assets) -> Self {
        // Preload icons
        let folder_icon = assets.load(AssetName::new("EDITOR_ASSETS/folder.tex"));
        let file_icon = assets.load(AssetName::new("EDITOR_ASSETS/file.tex"));
        assets.wait_for_load(&folder_icon);
        assets.wait_for_load(&file_icon);

        AssetViewer {
            assets: assets.clone(),
            folder_icon,
            file_icon,
            root: Folder::new_root(assets),
            current: Vec::default(),
            loading: None,
        }
    }

    /// Imports a new asset or folder and registers it with the viewer.
    pub fn import(&mut self, path: &Path, assets: &Assets) {
        // No-op if we're already loading
        if self.loading.is_some() {
            return;
        }

        // No-op if it's a symbolic link
        if path.is_symlink() {
            return;
        }

        let path: PathBuf = path.into();
        let cur_folder = self.path_to_cur_folder();
        let assets_cl = assets.clone();
        self.loading = Some(ParTask::new(move || {
            // Determine the destination name
            let mut dst = PathBuf::from("./assets/game/");
            dst.push(cur_folder);
            dst.push(path.file_name().unwrap().to_str().unwrap());

            // Find an unused name if needed
            if dst.exists() {
                let mut i = 1;
                loop {
                    let stem = dst.file_stem().unwrap().to_str().unwrap();
                    let new_name = match dst.extension() {
                        Some(ext) => format!("{} {}.{}", stem, i, ext.to_str().unwrap()),
                        None => format!("{} {}", stem, i),
                    };

                    let mut new_dst = dst.clone();
                    new_dst.set_file_name(new_name);

                    if new_dst.exists() {
                        i += 1
                    } else {
                        dst = new_dst;
                        break;
                    }
                }
            }

            // Copy the file/directory
            if path.is_dir() {
                fs_extra::dir::copy(
                    path,
                    &dst,
                    &fs_extra::dir::CopyOptions {
                        overwrite: false,
                        skip_exist: true,
                        buffer_size: 4096,
                        copy_inside: true,
                        content_only: false,
                        depth: 0,
                    },
                )
                .unwrap();
            } else if path.is_file() {
                std::fs::copy(path, &dst).unwrap();
            }

            // Helper to register an asset
            fn register_asset(assets: &Assets, name: &AssetName) {
                // Construct the asset name
                let mut asset_name = AssetNameBuf::new();
                for item in name.iter().skip(3) {
                    asset_name.push(item);
                }

                // Scan for the asset
                if !assets.scan_for(&asset_name) {
                    warn!("Scan failed for imported asset {:?}.", &asset_name);
                }
            }

            // Register the asset
            if dst.is_file() {
                register_asset(&assets_cl, &dst);
            }
            // Scan recursively for assets
            else if dst.is_dir() {
                fn find_assets(assets: &Assets, dir: &Path) {
                    let dir: PathBuf = dir.into();

                    for entry in dir.read_dir().unwrap() {
                        let entry = entry.unwrap();
                        let meta = entry.metadata().unwrap();

                        if meta.is_file() {
                            register_asset(assets, &entry.path());
                        } else if meta.is_dir() {
                            find_assets(assets, &entry.path());
                        }
                    }
                }

                find_assets(&assets_cl, &dst);
            }

            Ok(dst)
        }));
    }

    /// Computes the path to the current folder.
    fn path_to_cur_folder(&self) -> PathBuf {
        let mut path = PathBuf::from("./");
        for cur in &self.current {
            path.push(cur);
        }
        path
    }
}

impl View for AssetViewer {
    fn show(
        &mut self,
        ui: &imgui::Ui,
        _controller: &mut crate::controller::Controller,
        resc: &mut crate::editor::Resources,
    ) {
        // Asset loading window
        if let Some(loading) = &mut self.loading {
            let mut done = false;

            // It should be possible to close the loading dialouge if there was an error
            let mut can_close = match loading.get() {
                ParTaskGet::Err(_) => true,
                ParTaskGet::Panic(_) => true,
                _ => false,
            };

            // Used to detect if the window is closed
            let old_can_close = can_close;

            let mut window = ui
                .window("Loading Asset")
                .size([320.0, 100.0], imgui::Condition::Always);

            if can_close {
                window = window.opened(&mut can_close);
            }

            window.build(|| {
                loading.ui(ui, |_asset| {
                    // Rescan the project directory
                    self.root = Folder::new_root(resc.assets);
                    done = true;
                });
            });

            if done || !can_close && old_can_close {
                self.loading = None;
            }
        }

        ui.disabled(self.loading.is_some(), || {
            let style = unsafe { ui.style() };
            let mut new_folder = None;
            ui.window("Assets").build(|| {
                if ui.button("Refresh") {
                    // Rescan the project directory
                    self.root = Folder::new_root(resc.assets);
                }

                ui.same_line();
                ui.text(" | ");
                ui.same_line();

                if ui.button("Root") {
                    self.current.clear();
                }
                ui.same_line();
                ui.text(">");

                // Find the current folder
                let mut current_folder = &self.root;
                for (i, folder_name) in self.current.iter().enumerate() {
                    // Draw the folder on the tab
                    ui.same_line();
                    if ui.button(folder_name) {
                        self.current.truncate(i + 1);
                        break;
                    }
                    ui.same_line();
                    ui.text(">");

                    // Find the index of the sub folder
                    let mut idx = None;
                    for (i, folder) in current_folder.folders.iter().enumerate() {
                        if folder.name == *folder_name {
                            idx = Some(i);
                            break;
                        }
                    }

                    match idx {
                        Some(idx) => {
                            current_folder = &current_folder.folders[idx];
                        }
                        // We didn't find the sub folder, so go back to the root and stop searching
                        None => {
                            current_folder = &self.root;
                            self.current.clear();
                            break;
                        }
                    }
                }
                ui.separator();

                // Folders
                let folder_icon_id = self.assets.get(&self.folder_icon).unwrap().texture.ui_id();
                for (i, folder) in current_folder.folders.iter().enumerate() {
                    let last =
                        i == current_folder.folders.len() - 1 && current_folder.assets.is_empty();
                    let image = folder_icon_id;

                    // Move to next folder if selected
                    if asset_button(
                        ui,
                        style,
                        image,
                        None,
                        &folder.display_name,
                        &folder.name,
                        last,
                    ) {
                        new_folder = Some(folder.name.clone());
                    }
                }

                // Assets
                let file_icon_id = self.assets.get(&self.file_icon).unwrap().texture.ui_id();
                for (i, asset) in current_folder.assets.iter().enumerate() {
                    let last = i == current_folder.assets.len() - 1;
                    let image = file_icon_id;

                    let id = resc
                        .assets
                        .get_id_by_name(&asset.asset_name)
                        .map(|id| RawHandle { id });

                    if asset_button(
                        ui,
                        style,
                        image,
                        id,
                        &asset.display_name,
                        asset.asset_name.to_str().unwrap(),
                        last,
                    ) {
                        resc.ecs_commands
                            .events
                            .submit(InspectorItem::Asset(asset.asset_name.clone()));
                    }
                }
            });

            if let Some(new_folder) = new_folder {
                self.current.push(new_folder);
            }
        });
    }
}

impl Folder {
    fn new(name: String) -> Self {
        let display_name = shorten(Cow::Borrowed(&name)).into_owned();

        Folder {
            name,
            display_name,
            folders: Vec::default(),
            assets: Vec::default(),
        }
    }

    fn new_root(assets: &Assets) -> Self {
        let mut root = Folder::default();

        // First pass to register assets
        for pair in assets.assets() {
            let asset = pair.value();
            let name = asset.name().file_name().unwrap();

            // Skip if editor assets or fallback
            if asset.name().iter().next().unwrap() == EDITOR_ASSET_FOLDER_NAME {
                continue;
            }

            if asset.name().iter().next().unwrap() == FALLBACK_FOLDER_NAME {
                continue;
            }

            // Skip if meta file
            if let Some(ext) = asset.name().extension() {
                if ext == "meta" {
                    continue;
                }
            }

            // Skip if the file doesn't exist
            let mut abs_path = PathBuf::from("./assets/game/");
            abs_path.push(asset.name());
            if !abs_path.exists() {
                continue;
            }

            // First, construct all the folders of the asset
            let mut current_dir = &mut root;

            'outer: for folder in asset.name().iter() {
                // Skip if root
                if folder.is_empty() || folder == "." || folder == "/" {
                    continue;
                }

                // Skip if actual asset
                if folder == name {
                    continue;
                }

                // Skip if we already have the folder
                let folder_name = folder.to_str().unwrap();
                for (i, cur_folder) in current_dir.folders.iter().enumerate() {
                    if cur_folder.name == folder_name {
                        current_dir = &mut current_dir.folders[i];
                        continue 'outer;
                    }
                }

                // Add the folder
                current_dir
                    .folders
                    .push(Folder::new(String::from(folder_name)));
                current_dir = current_dir.folders.last_mut().unwrap();
            }

            current_dir.assets.push(Asset {
                display_name: shorten(Cow::Owned(String::from(name.to_str().unwrap()))).into(),
                asset_name: AssetNameBuf::from(asset.name()),
            });
        }

        // Second pass to detect empty folders
        fn find_dirs(folders: &mut Vec<Folder>, dir: &Path) {
            // Find unadded directories
            'outer: for dir in dir.read_dir().unwrap() {
                let dir = dir.unwrap();
                let meta_data = dir.metadata().unwrap();

                // Skip if not a directory
                if !meta_data.is_dir() {
                    continue;
                }

                // Skip if symlink
                if meta_data.is_symlink() {
                    continue;
                }

                let dir_name: String = dir.file_name().to_str().unwrap().into();

                // Skip if we already have this folder
                for folder in folders.iter() {
                    if folder.name == dir_name {
                        continue 'outer;
                    }
                }

                folders.push(Folder::new(dir_name));
            }

            // Recurse on all folders
            for folder in folders.iter_mut() {
                let mut dir: PathBuf = dir.into();
                dir.push(&folder.name);
                find_dirs(&mut folder.folders, &dir);
            }
        }

        // Find empty folders
        find_dirs(&mut root.folders, &Path::new("./assets/game/"));

        root
    }
}

/// Shorts a string with an ellipses if it's too long.
#[inline]
fn shorten(mut s: Cow<str>) -> Cow<str> {
    if s.len() > MAX_CHARS_IN_ASSET {
        let mut short: String = s.into_owned().chars().take(MAX_CHARS_IN_ASSET).collect();
        short.push_str("...");
        s = Cow::Owned(short);
    }
    s
}

/// Draws a labeled image button that represents an asset.
#[inline]
fn asset_button(
    ui: &imgui::Ui,
    style: &imgui::Style,
    image: imgui::TextureId,
    asset: Option<RawHandle>,
    name: &str,
    id: &str,
    last_button: bool,
) -> bool {
    let mut clicked = false;
    ui.group(|| {
        let id = ui.push_id(id);
        let style = ui.push_style_color(imgui::StyleColor::Button, [0.0, 0.0, 0.0, 0.0]);
        // Asset icon/button
        clicked = imgui::ImageButton::new(image, [ASSET_ICON_SIZE, ASSET_ICON_SIZE]).build(ui);
        id.end();
        style.end();

        // Draw the name of the asset
        let button_left = ui.item_rect_min()[0];
        let button_right = ui.item_rect_max()[0];
        let text_width = ui.calc_text_size(&name)[0];
        let indent = (button_left + button_right - text_width) * 0.5;
        ui.align_text_to_frame_padding();
        ui.set_cursor_screen_pos([indent, ui.cursor_pos()[1] + ui.window_pos()[1]]);
        ui.text(&name);
    });

    if let Some(asset) = asset {
        if let Some(tooltip) = ui
            .drag_drop_source_config("Asset")
            .flags(imgui::DragDropFlags::SOURCE_ALLOW_NULL_ID)
            .begin_payload(DragDropPayload::Asset(asset))
        {
            ui.text(name);
            tooltip.end();
        }
    }

    let window_visible_x2 = ui.window_pos()[0] + ui.window_content_region_max()[0];
    let last_button_x2 = ui.item_rect_max()[0];
    let next_button_x2 = last_button_x2 + style.item_spacing[0] + ASSET_ICON_SIZE;

    if !last_button && next_button_x2 < window_visible_x2 {
        ui.same_line();
    }

    clicked
}
