use anyhow::Result;
use ard_engine::{assets::prelude::*, ecs::prelude::*, log::warn, render::model::ModelAsset};

use crate::{
    assets::meta::{MetaData, MetaFile},
    command::{instantiate::InstantiateCommand, EditorCommands},
};

use super::{EditorTask, TaskConfirmation};

pub struct InstantiateTask {
    asset: MetaFile,
    assets: Assets,
    handle: Option<InstantiateAssetHandle>,
}

pub enum InstantiateAssetHandle {
    Model(Handle<ModelAsset>),
}

impl InstantiateTask {
    pub fn new(asset: MetaFile, assets: Assets) -> Self {
        Self {
            asset,
            assets,
            handle: None,
        }
    }
}

impl EditorTask for InstantiateTask {
    fn has_confirm_ui(&self) -> bool {
        false
    }

    fn confirm_ui(&mut self, _ui: &mut egui::Ui) -> Result<TaskConfirmation> {
        unreachable!()
    }

    fn run(&mut self) -> anyhow::Result<()> {
        match &self.asset.data {
            MetaData::Model => {
                let handle = match self.assets.load::<ModelAsset>(&self.asset.baked) {
                    Some(handle) => handle,
                    None => {
                        return Err(anyhow::Error::msg(format!(
                            "could not load {:?}",
                            self.asset.baked
                        )))
                    }
                };
                self.assets.wait_for_load(&handle);
                self.handle = Some(InstantiateAssetHandle::Model(handle));
            }
            _ => {}
        }

        Ok(())
    }

    fn complete(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> Result<()> {
        let handle = match self.handle.take() {
            Some(handle) => handle,
            None => {
                warn!("Finished loading asset, but did not get a handle back.");
                return Ok(());
            }
        };

        res.get_mut::<EditorCommands>()
            .unwrap()
            .submit(InstantiateCommand::new(handle));

        Ok(())
    }
}
