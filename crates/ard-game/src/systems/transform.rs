use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_math::*;
use ard_render::system::RenderSystem;
use ard_render_objects::Model;

use crate::components::{
    destroy::Destroy,
    transform::{Children, Parent, Position, Rotation, Scale, SetParent},
};

use super::destroy::Destroyer;

#[derive(SystemState, Default)]
pub struct TransformUpdate {
    /// Entities with transforms, sorted based on depth in the hierarchy.
    hierarchy: Vec<Entity>,
}

type TransformUpdateQuery = (
    Read<Destroy>,
    Read<SetParent>,
    Read<Position>,
    Read<Rotation>,
    Read<Scale>,
    Write<Model>,
    Write<Parent>,
    Write<Children>,
);

impl TransformUpdate {
    pub fn on_tick(
        &mut self,
        _: Tick,
        commands: Commands,
        queries: Queries<TransformUpdateQuery>,
        _: Res<()>,
    ) {
        // Need to remove destroyed entities from child lists
        for (entity, parent) in queries
            .filter()
            .with::<Destroy>()
            .make::<(Entity, Read<Parent>)>()
        {
            if let Some(mut children) = queries.get::<Write<Children>>(parent.0) {
                children.0.retain(|e| *e != entity);
            }
        }

        // Remove from old parents child list if we changed parents
        for (entity, (parent, new)) in
            queries.make::<(Entity, (Option<Write<Parent>>, Read<SetParent>))>()
        {
            // Remove self from old parents child list and update parent
            match parent {
                Some(parent) => {
                    if let Some(mut children) = queries.get::<Write<Children>>(parent.0) {
                        children.0.retain(|e| *e != entity);
                    }

                    if let Some(new) = new.0 {
                        parent.0 = new;
                    }
                }
                None => {
                    if let Some(new) = new.0 {
                        commands.entities.add_component(entity, Parent(new));
                    }
                }
            }

            // Put self into new parents child list
            if let Some(parent) = new.0 {
                if let Some(mut children) = queries.get::<Write<Children>>(parent) {
                    children.0.push(entity);
                }
            }

            commands.entities.remove_component::<SetParent>(entity);
        }

        // Find the roots of the transform hierarchy and initialize their model matrices
        self.hierarchy.clear();

        // This first query is every component without a parent and without a 'SetComponent'
        // marker. Obviously, these are roots.
        let guaranteed_roots = queries
            .filter()
            .without::<Destroy>()
            .without::<Parent>()
            .make::<(
                Entity,
                (
                    Option<Read<Position>>,
                    Option<Read<Rotation>>,
                    Option<Read<Scale>>,
                    Write<Model>,
                ),
            )>();
        self.hierarchy.reserve(guaranteed_roots.len());

        for (entity, (pos, rot, scl, model)) in guaranteed_roots {
            self.hierarchy.push(entity);

            let pos = pos.map(|p| p.0).unwrap_or(Vec3A::ZERO);
            let rot = rot.map(|r| r.0).unwrap_or(Quat::IDENTITY);
            let scl = scl.map(|s| s.0).unwrap_or(Vec3A::ONE);

            model.0 = Mat4::from_scale_rotation_translation(scl.into(), rot, pos.into());
        }

        // This second query is every component with a `SetComponent` marker and a parent. They
        // "may" be setting the component to `None` and are thus a root.
        let possible_roots = queries
            .filter()
            .without::<Destroy>()
            .with::<Parent>()
            .make::<(
                Entity,
                (
                    Read<SetParent>,
                    Option<Read<Position>>,
                    Option<Read<Rotation>>,
                    Option<Read<Scale>>,
                    Write<Model>,
                ),
            )>();
        self.hierarchy.reserve(possible_roots.len());

        for (entity, (set_parent, pos, rot, scl, model)) in possible_roots {
            if set_parent.0.is_some() {
                continue;
            }

            self.hierarchy.push(entity);

            let pos = pos.map(|p| p.0).unwrap_or(Vec3A::ZERO);
            let rot = rot.map(|r| r.0).unwrap_or(Quat::IDENTITY);
            let scl = scl.map(|s| s.0).unwrap_or(Vec3A::ONE);

            model.0 = Mat4::from_scale_rotation_translation(scl.into(), rot, pos.into());
        }

        // Construct the tree from roots to leaves breadth first
        let mut i = 0;
        while i != self.hierarchy.len() {
            let entity = self.hierarchy[i];

            let parent_global = match queries.get::<Read<Model>>(entity) {
                Some(parent) => parent.0,
                None => {
                    i += 1;
                    continue;
                }
            };

            // Get the children of the current node
            let children = match queries.get::<Read<Children>>(entity) {
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

                let mut query = queries.get::<(
                    Option<Read<Position>>,
                    Option<Read<Rotation>>,
                    Option<Read<Scale>>,
                    Write<Model>,
                )>(*child);

                if let Some((pos, rot, scl, model)) = query.as_deref_mut() {
                    let pos = pos.map(|p| p.0).unwrap_or(Vec3A::ZERO);
                    let rot = rot.map(|r| r.0).unwrap_or(Quat::IDENTITY);
                    let scl = scl.map(|s| s.0).unwrap_or(Vec3A::ONE);

                    model.0 = parent_global
                        * Mat4::from_scale_rotation_translation(scl.into(), rot, pos.into());
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
            .run_before::<Tick, RenderSystem>()
            .build()
    }
}
