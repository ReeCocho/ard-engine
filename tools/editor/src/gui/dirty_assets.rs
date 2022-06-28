use std::collections::HashMap;

use crate::{editor_job::EditorJob, AssetMeta};
use ard_engine::{
    assets::prelude::{AssetName, AssetNameBuf, Assets, Handle},
    log::warn,
};

#[derive(Default)]
pub struct DirtyAssets {
    dirty: HashMap<AssetNameBuf, Handle<AssetMeta>>,
}

impl DirtyAssets {
    pub fn add(&mut self, name: &AssetName, handle: Handle<AssetMeta>) {
        self.dirty.insert(name.into(), handle);
    }

    /// Returns an editor job that saves all assets.
    pub fn flush(&mut self, assets: &Assets) -> EditorJob {
        let mut dirty = std::mem::take(&mut self.dirty);
        let asset_count = dirty.len();
        let mut current_asset_count = 0;
        let mut current_asset = AssetNameBuf::default();
        let assets_cl = assets.clone();
        let (send, recv) = crossbeam_channel::bounded(dirty.len());

        EditorJob::new(
            "Saving",
            Some((320, 80)),
            move || {
                for (name, handle) in dirty.drain() {
                    send.send(name.clone()).unwrap();

                    let mut asset = match assets_cl.get_mut(&handle) {
                        Some(asset) => asset,
                        None => {
                            warn!("attempt to flush meta `{:?}` but it was unloaded.", &name);
                            continue;
                        }
                    };

                    asset.save(&assets_cl);
                }
            },
            move |ui| {
                if let Ok(new_asset) = recv.try_recv() {
                    current_asset = new_asset;
                    current_asset_count += 1;
                }

                ui.text("Saving...");
                ui.text(format!("{:?}", &current_asset));
                imgui::ProgressBar::new(current_asset_count as f32 / asset_count as f32)
                    .overlay_text(format!("{}/{}", current_asset_count, asset_count))
                    .build(ui);
            },
        )
    }
}
