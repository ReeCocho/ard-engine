use ard_engine::{
    core::core::Tick,
    ecs::prelude::*,
    game::{
        components::transform::{Children, Parent},
        systems::transform::TransformUpdate,
    },
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
        queries: Queries<(Read<Children>,)>,
        res: Res<(Write<SceneGraph>,)>,
    ) {
        let mut scene_graph = res.get_mut::<SceneGraph>().unwrap();
        scene_graph.roots.clear();
        queries
            .filter()
            .without::<Parent>()
            .make::<(Entity, (Read<Children>,))>()
            .into_iter()
            .for_each(|(e, _)| {
                scene_graph.roots.push(e);
            });
    }
}

impl SceneGraph {
    #[inline]
    pub fn roots(&self) -> &[Entity] {
        &self.roots
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
            .run_after::<Tick, TransformUpdate>()
            .build()
    }
}
