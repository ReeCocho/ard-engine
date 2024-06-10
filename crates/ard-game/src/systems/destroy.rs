use ard_core::prelude::*;
use ard_ecs::prelude::*;

use crate::components::{destroy::Destroy, transform::Children};

#[derive(Debug, Default, SystemState)]
pub struct Destroyer {
    /// Cache to hold soon to be destroyed entities.
    to_destroy: Vec<Entity>,
}

type DestroySysQueries = (Read<Destroy>, Read<Children>);

impl Destroyer {
    fn on_tick(
        &mut self,
        _: Tick,
        commands: Commands,
        queries: Queries<DestroySysQueries>,
        _: Res<()>,
    ) {
        let query = queries.make::<(Entity, Read<Destroy>)>();

        self.to_destroy.clear();
        self.to_destroy.reserve(query.len());

        for (entity, _) in query {
            Self::destroy_recurse(entity, &queries, &mut self.to_destroy);
        }

        if !self.to_destroy.is_empty() {
            commands.entities.destroy(&self.to_destroy);
        }
    }

    fn destroy_recurse(
        entity: Entity,
        queries: &Queries<DestroySysQueries>,
        to_destroy: &mut Vec<Entity>,
    ) {
        to_destroy.push(entity);
        let children = queries.get::<Read<Children>>(entity);
        if let Some(children) = children {
            children.0.iter().for_each(|e| {
                Self::destroy_recurse(*e, queries, to_destroy);
            });
        }
    }
}

impl From<Destroyer> for System {
    fn from(sys: Destroyer) -> Self {
        SystemBuilder::new(sys)
            .with_handler(Destroyer::on_tick)
            .build()
    }
}
