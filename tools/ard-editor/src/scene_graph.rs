use ard_engine::{
    core::core::Tick,
    ecs::prelude::*,
    transform::{system::TransformHierarchyUpdate, Children, Parent, SetParent},
};

#[derive(Resource, Default)]
pub struct SceneGraph {
    roots: Vec<Entity>,
}

#[derive(SystemState)]
pub struct DiscoverSceneGraphRoots;

impl DiscoverSceneGraphRoots {
    fn tick(
        &mut self,
        _: Tick,
        _: Commands,
        queries: Queries<(Read<Children>, Read<SetParent>)>,
        res: Res<(Write<SceneGraph>,)>,
    ) {
        let mut scene_graph = res.get_mut::<SceneGraph>().unwrap();

        // Remove entities that are destroyed
        scene_graph
            .roots_mut()
            .retain(|root| queries.is_alive(*root));

        // Add entities if they are new
        queries
            .filter()
            .without::<Parent>()
            .make::<(Entity, (Read<Children>,))>()
            .into_iter()
            .for_each(|(e, _)| {
                if !scene_graph.roots.contains(&e) {
                    scene_graph.roots.push(e);
                }
            });

        // Insert entities into the correct spot if their parent was updated
        queries
            .make::<(Entity, (Read<SetParent>,))>()
            .into_iter()
            .for_each(|(e, (new_parent,))| {
                let mut index = None;
                for (i, entity) in scene_graph.roots.iter().enumerate() {
                    if *entity == e {
                        index = Some(i);
                        break;
                    }
                }

                if let Some(index) = index {
                    scene_graph.roots.remove(index);
                }

                if new_parent.new_parent.is_some() {
                    return;
                }

                let new_index = new_parent.index.min(scene_graph.roots.len());
                scene_graph.roots.insert(new_index, e);
            });
    }
}

impl SceneGraph {
    #[inline]
    pub fn roots(&self) -> &[Entity] {
        &self.roots
    }

    pub fn find_in_roots(&self, target: Entity) -> Option<usize> {
        let mut index = None;
        for (i, entity) in self.roots.iter().enumerate() {
            if *entity == target {
                index = Some(i);
                break;
            }
        }
        index
    }

    #[inline]
    pub fn all_entities(&self, queries: &Queries<Everything>) -> Vec<Entity> {
        Self::collect_children(queries, self.roots.clone())
    }

    pub fn collect_children(queries: &Queries<Everything>, mut roots: Vec<Entity>) -> Vec<Entity> {
        let mut i = 0;
        while i < roots.len() {
            let entity = roots[i];
            let children = match queries.get::<Read<Children>>(entity) {
                Some(children) => children,
                None => {
                    i += 1;
                    continue;
                }
            };

            children.0.iter().for_each(|c| roots.push(*c));
            i += 1;
        }
        roots
    }

    #[inline]
    pub fn roots_mut(&mut self) -> &mut Vec<Entity> {
        &mut self.roots
    }

    #[inline]
    pub fn add_roots(&mut self, new_roots: impl Iterator<Item = Entity>) {
        self.roots.extend(new_roots);
    }
}

impl From<DiscoverSceneGraphRoots> for System {
    fn from(value: DiscoverSceneGraphRoots) -> Self {
        SystemBuilder::new(value)
            .with_handler(DiscoverSceneGraphRoots::tick)
            .run_after::<Tick, TransformHierarchyUpdate>()
            .build()
    }
}
