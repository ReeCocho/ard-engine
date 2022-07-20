use std::process::Child;

use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_graphics_api::prelude::*;
use ard_math::*;

use crate::{
    components::transform::{Children, Parent, PrevParent, Transform},
    destroy::{Destroy, Destroyer},
};

#[derive(SystemState, Default)]
pub struct TransformUpdate {
    /// Entities with transforms, sorted based on depth in the hierarchy.
    hierarchy: Vec<Entity>,
}

impl TransformUpdate {
    pub fn on_tick(
        &mut self,
        _: Tick,
        commands: Commands,
        queries: Queries<(
            Read<Transform>,
            Read<Parent>,
            Read<Destroy>,
            Write<Children>,
            Write<Model>,
            Write<PrevParent>,
        )>,
        _: Res<()>,
    ) {
        // Need to remove destroyed entities from child lists
        for (entity, parent) in queries
            .filter()
            .with::<Destroy>()
            .make::<(Entity, Read<Parent>)>()
        {
            if let Some(parent) = parent.0 {
                if let Some(mut children) = queries.get::<Write<Children>>(parent) {
                    children.0.retain(|e| *e != entity);
                }
            }
        }

        // Remove from old parents child list if we changed parents
        for (entity, (parent, prev)) in queries.make::<(Entity, (Read<Parent>, Read<PrevParent>))>()
        {
            // Remove self from old parents list
            if let Some(prev) = prev.0 {
                if let Some(mut children) = queries.get::<Write<Children>>(prev) {
                    children.0.retain(|e| *e != entity);
                }
            }

            // Put self into new parents list
            if let Some(parent) = parent.0 {
                if let Some(mut children) = queries.get::<Write<Children>>(parent) {
                    children.0.push(entity);
                }
            }

            commands.entities.remove_component::<PrevParent>(entity);
        }

        let current = queries
            .filter()
            .without::<Destroy>()
            .make::<(Entity, (Read<Parent>, Read<Transform>, Write<Model>))>();

        self.hierarchy.clear();
        self.hierarchy.reserve(current.len());

        // Find all transforms with no parents (the roots of the tree)
        for (entity, (parent, transform, global)) in current {
            if parent.0.is_none() {
                self.hierarchy.push(entity);
                global.0 = Mat4::from_scale_rotation_translation(
                    transform.scale(),
                    transform.rotation(),
                    transform.position(),
                );
            }
        }

        // Construct the tree from roots to leaves breadth first
        let mut i = 0;
        while i != self.hierarchy.len() {
            let parent_global = match queries.get::<Read<Model>>(self.hierarchy[i]) {
                Some(parent) => parent.0,
                None => {
                    i += 1;
                    continue;
                }
            };

            // Get the children of the current node
            let children = match queries.get::<Read<Children>>(self.hierarchy[i]) {
                Some(children) => children,
                None => {
                    i += 1;
                    continue;
                }
            };

            // Add the child to the hierarchy and compute their global transforms from the
            // parent
            for child in children.0.iter() {
                self.hierarchy.push(*child);
                if let Some(mut query) = queries.get::<(Read<Transform>, Write<Model>)>(*child) {
                    query.1 .0 = parent_global
                        * Mat4::from_scale_rotation_translation(
                            query.0.scale(),
                            query.0.rotation(),
                            query.0.position(),
                        );
                }
            }

            i += 1;
        }
    }
}

impl From<TransformUpdate> for System {
    fn from(sys: TransformUpdate) -> Self {
        SystemBuilder::new(sys)
            .with_handler(TransformUpdate::on_tick)
            .run_before::<Tick, Destroyer>()
            .build()
    }
}
