use ard_engine::{
    ecs::prelude::*,
    game::{GameStart, GameStop},
};

use crate::{assets::EditorAssets, scene_graph::SceneGraph};

use super::{load::LoadSceneTask, save::SaveSceneTask, EditorTask, TaskConfirmation, TaskQueue};

pub struct StartPlayTask {
    save_task: SaveSceneTask,
}

pub struct StopPlayTask {}

impl StartPlayTask {
    pub fn new(save_task: SaveSceneTask) -> Self {
        Self { save_task }
    }
}

impl StopPlayTask {
    pub fn new() -> Self {
        Self {}
    }
}

impl EditorTask for StartPlayTask {
    fn has_confirm_ui(&self) -> bool {
        self.save_task.has_confirm_ui()
    }

    fn confirm_ui(&mut self, ui: &mut egui::Ui) -> anyhow::Result<TaskConfirmation> {
        self.save_task.confirm_ui(ui)
    }

    fn pre_run(
        &mut self,
        commands: &Commands,
        queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> anyhow::Result<()> {
        self.save_task.pre_run(commands, queries, res)
    }

    fn run(&mut self) -> anyhow::Result<()> {
        self.save_task.run()
    }

    fn complete(
        &mut self,
        commands: &Commands,
        queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> anyhow::Result<()> {
        self.save_task.complete(commands, queries, res)?;
        commands.events.submit(GameStart);
        Ok(())
    }
}

impl EditorTask for StopPlayTask {
    fn has_confirm_ui(&self) -> bool {
        false
    }

    fn confirm_ui(&mut self, _ui: &mut egui::Ui) -> anyhow::Result<TaskConfirmation> {
        Ok(TaskConfirmation::Ready)
    }

    fn run(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn complete(
        &mut self,
        commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) -> anyhow::Result<()> {
        commands.events.submit(GameStop);

        let asset = res
            .get::<SceneGraph>()
            .unwrap()
            .active_scene()
            .and_then(|name| res.get::<EditorAssets>().unwrap().find_asset(name).cloned());

        match asset {
            Some(asset) => {
                res.get_mut::<TaskQueue>()
                    .unwrap()
                    .add(LoadSceneTask::new_no_confirm(&asset));
            }
            None => todo!("Load an empty scene on error here"),
        }

        Ok(())
    }
}
