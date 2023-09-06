use ard_ecs::prelude::*;
use ard_render_base::ecs::RenderPreprocessing;

use crate::factory::Factory;

#[derive(SystemState)]
pub struct FactoryProcessingSystem;

impl FactoryProcessingSystem {
    fn process(
        &mut self,
        evt: RenderPreprocessing,
        _: Commands,
        _: Queries<()>,
        res: Res<(Write<Factory>,)>,
    ) {
        let factory = res.get::<Factory>().unwrap();
        factory.process(evt.0);
    }
}

impl From<FactoryProcessingSystem> for System {
    fn from(value: FactoryProcessingSystem) -> Self {
        SystemBuilder::new(value)
            .with_handler(FactoryProcessingSystem::process)
            .build()
    }
}
