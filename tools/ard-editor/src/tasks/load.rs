use crate::{
    assets::EditorAsset, command::EditorCommands, gui::util, scene_graph::SceneGraph, ser,
};

use super::{EditorTask, TaskConfirmation, TaskState};
use ard_engine::{
    assets::prelude::*, core::prelude::*, ecs::prelude::*, game::save_data::SceneAsset,
    save_load::format::Ron,
};
use camino::Utf8PathBuf;

pub struct LoadSceneTask {
    file_name: Utf8PathBuf,
    meta_path: Utf8PathBuf,
    asset_path: AssetNameBuf,
    assets: Option<Assets>,
    handle: Option<Handle<SceneAsset>>,
    confirm: bool,
    state: TaskState,
}

impl LoadSceneTask {
    pub fn new(asset: &EditorAsset) -> Self {
        Self {
            file_name: asset.raw_path(),
            meta_path: asset.meta_path().into(),
            asset_path: asset.meta_file().baked.clone(),
            assets: None,
            handle: None,
            confirm: true,
            state: TaskState::new(format!("Loading {:?}", asset.raw_path())),
        }
    }

    pub fn new_no_confirm(asset: &EditorAsset) -> Self {
        Self {
            file_name: asset.raw_path(),
            meta_path: asset.meta_path().into(),
            asset_path: asset.meta_file().baked.clone(),
            assets: None,
            handle: None,
            confirm: false,
            state: TaskState::new(format!("Loading {:?}", asset.raw_path())),
        }
    }
}

impl EditorTask for LoadSceneTask {
    fn has_confirm_ui(&self) -> bool {
        self.confirm
    }

    fn confirm_ui(&mut self, ui: &mut egui::Ui) -> anyhow::Result<TaskConfirmation> {
        ui.label(format!(
            "Are you sure you want to load, `{}`? All unsaved progress will be lost.",
            self.file_name
        ));

        if ui.add(util::transformation_button("Yes")).clicked() {
            return Ok(TaskConfirmation::Ready);
        }

        if ui.button("No").clicked() {
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
        self.assets = Some(res.get::<Assets>().unwrap().clone());
        Ok(())
    }

    fn run(&mut self) -> anyhow::Result<()> {
        let assets = self.assets.as_ref().unwrap();

        // If the scene was already loaded, it should be reloaded in case it was modified
        let is_loaded = assets.loaded(&self.asset_path);

        let handle = match assets.load::<SceneAsset>(&self.asset_path) {
            Some(handle) => handle,
            None => return Err(anyhow::Error::msg("Asset does not exist.")),
        };

        if is_loaded {
            assets.reload(&handle);
        }

        assets.wait_for_load(&handle);
        self.handle = Some(handle);

        Ok(())
    }

    fn complete(
        &mut self,
        commands: &Commands,
        queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> anyhow::Result<()> {
        let handle = match self.handle.take() {
            Some(handle) => handle,
            None => return Err(anyhow::Error::msg("Could not load scene.")),
        };

        // Instantiate the scene
        let assets = res.get::<Assets>().unwrap();
        let asset = match assets.get(&handle) {
            Some(asset) => asset,
            None => return Err(anyhow::Error::msg("Could not load scene.")),
        };

        ser::loader::<Ron>().load(asset.data().clone(), assets.clone(), &commands.entities)?;

        // Clear the command queue
        res.get_mut::<EditorCommands>()
            .unwrap()
            .reset_all(commands, queries, res);

        // Destroy every entity in the scene
        let mut scene_graph = res.get_mut::<SceneGraph>().unwrap();
        scene_graph
            .all_entities(queries)
            .into_iter()
            .for_each(|entity| {
                commands.entities.add_component(entity, Destroy);
            });

        // Set as the active scene
        scene_graph.set_active_scene(&self.meta_path);

        Ok(())
    }
}
