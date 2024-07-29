use std::{
    path::PathBuf,
    sync::{atomic::Ordering, Arc},
};

use ard_engine::{
    assets::{asset::AssetNameBuf, prelude::lof::LofBuildState},
    ecs::prelude::*,
    game::save_data::{InitialSceneAsset, INITIAL_SCENE_ASSET_NAME},
    log::info,
};
use path_macro::path;

use crate::{
    assets::{meta::AssetType, EditorAssets, EditorAssetsManifest},
    gui::{drag_drop::DragDropPayload, util},
};

use super::{EditorTask, TaskConfirmation, TaskState};

pub struct BuildGameTask {
    initial_scene: AssetNameBuf,
    active_package: PathBuf,
    active_package_manifest: Option<EditorAssetsManifest>,
    state: TaskState,
}

impl Default for BuildGameTask {
    fn default() -> Self {
        Self {
            initial_scene: AssetNameBuf::default(),
            active_package: PathBuf::default(),
            active_package_manifest: None,
            state: TaskState::new("Build Game"),
        }
    }
}

impl EditorTask for BuildGameTask {
    fn confirm_ui(&mut self, ui: &mut egui::Ui) -> anyhow::Result<TaskConfirmation> {
        ui.heading("Initial Scene");
        let (_, payload) = ui.dnd_drop_zone::<DragDropPayload, _>(egui::Frame::none(), |ui| {
            let mut tmp = self.initial_scene.as_str().to_owned();
            ui.add_enabled(false, egui::TextEdit::singleline(&mut tmp));
        });

        if let Some(payload) = payload {
            if let DragDropPayload::Asset(asset) = &*payload {
                if asset.meta_file().data.ty() == AssetType::Scene {
                    self.initial_scene = asset.meta_file().baked.clone();
                }
            }
        }

        let can_build = !self.initial_scene.as_str().is_empty();

        if ui
            .add_enabled(can_build, util::constructive_button("Build"))
            .clicked()
        {
            return Ok(TaskConfirmation::Ready);
        }

        if ui.button("Cancel").clicked() {
            return Ok(TaskConfirmation::Cancel);
        }

        Ok(TaskConfirmation::Wait)
    }

    fn state(&mut self) -> Option<TaskState> {
        Some(self.state.clone())
    }

    fn pre_run(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> anyhow::Result<()> {
        let assets = res.get::<EditorAssets>().unwrap();
        self.active_package = assets.active_package_root().into();
        self.active_package_manifest = Some(assets.build_manifest());
        Ok(())
    }

    fn run(&mut self) -> anyhow::Result<()> {
        let start = std::time::Instant::now();
        info!("Build started.");

        let manifest = self.active_package_manifest.take().unwrap();

        let mut manifest_name = PathBuf::from(self.active_package.file_name().unwrap());
        manifest_name.set_extension("manifest");
        let manifest_path = path!(self.active_package / manifest_name);

        let initial_scene_path = path!(self.active_package / INITIAL_SCENE_ASSET_NAME);
        let initial_scene = InitialSceneAsset {
            asset_name: self.initial_scene.clone(),
        };

        let mut lof_name = manifest_name.clone();
        lof_name.set_extension("lof");

        let f = std::fs::File::options()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&manifest_path)
            .unwrap();
        bincode::serialize_into(f, &manifest).unwrap();

        let f = std::fs::File::options()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&initial_scene_path)
            .unwrap();
        bincode::serialize_into(f, &initial_scene).unwrap();

        std::fs::create_dir_all("./build/packages/")?;
        let build_state = Arc::new(LofBuildState::default());

        let build_state_clone = build_state.clone();
        let out = path!("./build/packages" / lof_name);
        let src = self.active_package.clone();
        let build_thread = std::thread::spawn(move || {
            ard_engine::assets::package::lof::create_lof_from_folder_mt(
                out,
                src,
                8,
                build_state_clone,
            );
        });

        while !build_thread.is_finished() {
            let file_count = build_state.file_count.load(Ordering::Relaxed).max(1);
            let files_saved = build_state.files_saved.load(Ordering::Relaxed);
            self.state
                .set_completion(files_saved as f32 / file_count as f32);
        }
        let _ = build_thread.join();

        std::fs::remove_file(manifest_path)?;
        std::fs::remove_file(initial_scene_path)?;
        std::fs::write(
            "./build/packages/packages.ron",
            format!(
                "PackageList(packages: [ \"{}\" ])",
                lof_name.to_str().unwrap()
            ),
        )?;

        let end = std::time::Instant::now();
        info!("Build complete in {}s", end.duration_since(start).as_secs());

        Ok(())
    }

    fn complete(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        _res: &Res<Everything>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}
