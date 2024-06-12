use ard_engine::{ecs::prelude::*, render::EntitySelected};

#[derive(Resource, Default)]
pub enum Selected {
    #[default]
    None,
    Entity(Entity),
}

#[derive(SystemState)]
pub struct SelectEntitySystem;

impl SelectEntitySystem {
    fn selected_entity(
        &mut self,
        evt: EntitySelected,
        _: Commands,
        _: Queries<()>,
        res: Res<(Write<Selected>,)>,
    ) {
        let mut selected = res.get_mut::<Selected>().unwrap();
        *selected = Selected::Entity(evt.0);
    }
}

impl From<SelectEntitySystem> for System {
    fn from(value: SelectEntitySystem) -> Self {
        SystemBuilder::new(value)
            .with_handler(SelectEntitySystem::selected_entity)
            .build()
    }
}
