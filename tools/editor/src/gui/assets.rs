use std::borrow::Cow;

use ard_engine::{
    assets::prelude::*, ecs::prelude::*, graphics::prelude::*, graphics_assets::prelude::*, math::*,
};

use super::inspector::InspectorItem;

const MAX_CHARS_IN_ASSET: usize = 12;
const ASSET_ICON_SIZE: f32 = 80.0;
const EDITOR_ASSET_FOLDER_NAME: &'static str = "EDITOR_ASSETS";

pub struct AssetViewer {
    assets: Assets,
    folder_icon: Handle<TextureAsset>,
    file_icon: Handle<TextureAsset>,
    /// Root folder in the asset view.
    root: Folder,
    /// List of folder names in order that points to the current folder. The root is implied to be
    /// the first element.
    current: Vec<String>,
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
        }
    }

    pub fn draw(&mut self, ui: &mut imgui::Ui, commands: &Commands) {
        let style = unsafe { ui.style() };

        let mut new_folder = None;

        ui.window("Assets").build(|| {
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
                if asset_button(ui, style, image, &folder.display_name, &folder.name, last) {
                    new_folder = Some(folder.name.clone());
                }
            }

            // Assets
            let file_icon_id = self.assets.get(&self.file_icon).unwrap().texture.ui_id();
            for (i, asset) in current_folder.assets.iter().enumerate() {
                let last = i == current_folder.assets.len() - 1;
                let image = file_icon_id;

                if asset_button(
                    ui,
                    style,
                    image,
                    &asset.display_name,
                    asset.asset_name.to_str().unwrap(),
                    last,
                ) {
                    commands
                        .events
                        .submit(InspectorItem::Asset(asset.asset_name.clone()));
                }
            }
        });

        if let Some(new_folder) = new_folder {
            self.current.push(new_folder);
        }
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

        for asset in assets.assets().values() {
            let name = asset.name().file_name().unwrap();

            // Skip if editor assets
            if asset.name().iter().next().unwrap() == EDITOR_ASSET_FOLDER_NAME {
                continue;
            }

            // Skip if meta file
            if let Some(ext) = asset.name().extension() {
                if ext == "meta" {
                    continue;
                }
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

    let window_visible_x2 = ui.window_pos()[0] + ui.window_content_region_max()[0];
    let last_button_x2 = ui.item_rect_max()[0];
    let next_button_x2 = last_button_x2 + style.item_spacing[0] + ASSET_ICON_SIZE;

    if !last_button && next_button_x2 < window_visible_x2 {
        ui.same_line();
    }

    clicked
}
