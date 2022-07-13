use ard_core::prelude::*;
use ard_ecs::prelude::*;

/// Marks an entity for destruction. Destruction is deferred, which allows you to run code on
/// entities that are about to be destroyed. To do this, your system must have a `Tick` handler
/// that runs before the `Destroyer` system.
#[derive(Debug, Component, Copy, Clone)]
pub struct Destroy;

#[derive(Debug, Default, SystemState)]
pub struct Destroyer {
    /// Cache to hold soon to be destroyed entities.
    to_destroy: Vec<Entity>,
}

impl Destroyer {
    fn on_tick(
        &mut self,
        _: Tick,
        commands: Commands,
        queries: Queries<(Read<Destroy>,)>,
        _: Res<()>,
    ) {
        let query = queries.make::<(Entity, Read<Destroy>)>();

        self.to_destroy.clear();
        self.to_destroy.reserve(query.len());

        for (entity, _) in query {
            self.to_destroy.push(entity);
        }

        commands.entities.destroy(&self.to_destroy);
    }
}

impl From<Destroyer> for System {
    fn from(sys: Destroyer) -> Self {
        SystemBuilder::new(sys)
            .with_handler(Destroyer::on_tick)
            .build()
    }
}
