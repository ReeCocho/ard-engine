use ard_engine::{
    assets::prelude::*,
    ecs::prelude::*,
    game::{
        object::{static_object::StaticObject, GameObject},
        Scene, SceneDescriptor, SceneEntities, SceneGameObject,
    },
};

#[derive(Resource, Default)]
pub struct SceneGraph {
    roots: Vec<SceneGraphNode>,
}

pub struct SceneGraphNode {
    pub entity: Entity,
    pub children: Vec<SceneGraphNode>,
    pub ty: SceneGameObject,
}

impl SceneGraph {
    #[inline]
    pub fn roots(&self) -> &[SceneGraphNode] {
        &self.roots
    }

    pub fn save(&self, queries: &Queries<Everything>, assets: &Assets) -> SceneDescriptor {
        let mut entities = SceneEntities::default();

        fn traverse(entities: &mut SceneEntities, node: &SceneGraphNode) {
            // Add the entity to the correct list
            match &node.ty {
                SceneGameObject::StaticObject => entities.StaticObject_entities.push(node.entity),
            }

            // Traverse children
            for child in &node.children {
                traverse(entities, child);
            }
        }

        for root in &self.roots {
            traverse(&mut entities, root);
        }

        SceneDescriptor::new(entities, queries, assets)
    }

    pub fn create(&mut self, ty: SceneGameObject, commands: &EntityCommands) {
        let entity = match ty {
            SceneGameObject::StaticObject => StaticObject::create_default(commands),
        };

        self.roots.push(SceneGraphNode {
            entity,
            children: Vec::default(),
            ty,
        });
    }
}
