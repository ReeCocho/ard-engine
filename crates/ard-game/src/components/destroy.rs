use ard_ecs::{prelude::*, system::data::SystemData};

use super::transform::Children;

/// Marks an entity for destruction. Destruction is deferred, which allows you to run code on
/// entities that are about to be destroyed. To do this, your system must have a `Tick` handler
/// that runs before the `Destroyer` system.
///
/// You should use the `Destroy::entity` function to recursively destroy objects in the transform
/// hierarchy.
#[derive(Debug, Component, Copy, Clone)]
pub struct Destroy;

impl Destroy {
    /// NOTE: The provided `queries` must support `Read<Children>`.
    pub fn entity(entity: Entity, commands: &EntityCommands, queries: &Queries<impl SystemData>) {
        let mut to_destroy = vec![entity];
        let mut i = 0;

        while i != to_destroy.len() {
            let cur_entity = to_destroy[i];
            i += 1;

            // Mark for deletion
            commands.add_component(cur_entity, Destroy);

            // Append children
            let children = match queries.get::<Read<Children>>(cur_entity) {
                Some(children) => children,
                None => continue,
            };
            to_destroy.extend_from_slice(&children.0);
        }
    }
}
