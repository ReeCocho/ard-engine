use anyhow::Result;
use ard_engine::{assets::prelude::*, ecs::prelude::*};
use camino::Utf8Path;
use path_macro::path;

use crate::tasks::{EditorTask, TaskConfirmation};

use super::{meta::AssetType, EditorAssets};

pub trait AssetOp: Send + Sync {
    fn meta_path(&self) -> &Utf8Path;

    fn raw_path(&self) -> &Utf8Path;

    fn asset_ty(&self) -> AssetType;

    fn confirm_ui(&mut self, ui: &mut egui::Ui) -> Result<TaskConfirmation>;

    fn pre_run(&mut self, assets: &Assets, editor_assets: &EditorAssets) -> Result<()>;

    fn run(&mut self, is_shadowing: bool, is_leaf: bool) -> Result<()>;

    fn complete(
        &mut self,
        is_shadowing: bool,
        is_leaf: bool,
        assets: &Assets,
        editor_assets: &mut EditorAssets,
    ) -> Result<()>;
}

pub struct AssetOpInstance<A> {
    op: A,
    is_shadowing: bool,
    is_leaf: bool,
}

impl<A: AssetOp> AssetOpInstance<A> {
    pub fn new(op: A) -> Self {
        Self {
            op,
            is_shadowing: false,
            is_leaf: false,
        }
    }
}

impl<A: AssetOp> EditorTask for AssetOpInstance<A> {
    fn confirm_ui(&mut self, ui: &mut egui::Ui) -> Result<TaskConfirmation> {
        self.op.confirm_ui(ui)
    }

    fn pre_run(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> Result<()> {
        let assets = res.get::<Assets>().unwrap();
        let editor_assets = res.get::<EditorAssets>().unwrap();

        // Asset must be contained within the assets folder (i.e., in the active package)
        let path = path!(editor_assets.active_assets_root() / self.op.meta_path());
        if !path.exists() {
            return Err(anyhow::Error::msg("meta file does not exist"));
        }

        let path = path!(editor_assets.active_assets_root() / self.op.raw_path());
        if !path.exists() {
            return Err(anyhow::Error::msg("raw asset file does not exist"));
        }

        let asset = editor_assets
            .find_asset(self.op.meta_path())
            .ok_or(anyhow::Error::msg("could not find asset"))?;

        self.is_shadowing = asset.is_shadowing();
        self.is_leaf = asset.package() == editor_assets.active_package_id();

        self.op.pre_run(&assets, &editor_assets)
    }

    fn run(&mut self) -> Result<()> {
        self.op.run(self.is_shadowing, self.is_leaf)
    }

    fn complete(
        &mut self,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> Result<()> {
        let assets = res.get::<Assets>().unwrap();
        let mut editor_assets = res.get_mut::<EditorAssets>().unwrap();
        self.op
            .complete(self.is_shadowing, self.is_leaf, &assets, &mut editor_assets)?;

        Ok(())
    }
}
