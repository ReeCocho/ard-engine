use ard_engine::{ecs::prelude::*, log::*, window::prelude::WindowFileDropped};

use crate::{
    assets::meta::AssetType,
    tasks::{model::ModelImportTask, texture::TextureImportTask, TaskQueue},
};

#[derive(SystemState, Default)]
pub struct AssetImporter {}

impl AssetImporter {
    fn on_file_drop(
        &mut self,
        evt: WindowFileDropped,
        _: Commands,
        _: Queries<()>,
        res: Res<(Read<TaskQueue>,)>,
    ) {
        // Determine the assets type by its extension
        let ty = match AssetType::try_from(evt.file.as_path()) {
            Ok(ty) => ty,
            Err(err) => {
                warn!("{err:?}");
                return;
            }
        };

        // Submit import task to queue
        let task_queue = res.get::<TaskQueue>().unwrap();
        match ty {
            AssetType::Model => task_queue.add(ModelImportTask::new(evt.file)),
            AssetType::Texture => task_queue.add(TextureImportTask::new(evt.file)),
            _ => return,
        }
    }
}

impl From<AssetImporter> for System {
    fn from(value: AssetImporter) -> Self {
        SystemBuilder::new(value)
            .with_handler(AssetImporter::on_file_drop)
            .build()
    }
}
