use anyhow::Result;
use ard_engine::{assets::prelude::*, ecs::prelude::*};
use camino::Utf8PathBuf;
use path_macro::path;

use crate::tasks::{EditorTask, TaskConfirmation};

use super::{meta::AssetType, EditorAssets};

pub trait AssetOps: Send + Sync {
    fn meta_path(&self) -> &Utf8PathBuf;

    fn raw_path(&self) -> &Utf8PathBuf;

    fn asset_ty(&self) -> AssetType;

    fn confirm_ui(&mut self, ui: &mut egui::Ui) -> Result<TaskConfirmation>;

    fn pre_run(&mut self, assets: &Assets, editor_assets: &EditorAssets) -> Result<()>;

    fn run(&mut self, is_shadowing: bool, is_leaf: bool) -> Result<()>;

    fn complete(&mut self, assets: &Assets, editor_assets: &EditorAssets) -> Result<()>;
}

pub struct AssetOp<A> {
    op: A,
    is_shadowing: bool,
    is_leaf: bool,
}

impl<A: AssetOps> AssetOp<A> {
    pub fn new(op: A) -> Self {
        Self {
            op,
            is_shadowing: false,
            is_leaf: false,
        }
    }
}

impl<A: AssetOps> EditorTask for AssetOp<A> {
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

        self.is_shadowing = asset.is_shadowing;
        self.is_leaf = asset.package == editor_assets.active_package_id();

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
        let editor_assets = res.get::<EditorAssets>().unwrap();
        self.op.complete(&assets, &editor_assets)?;

        todo!()
    }
}
