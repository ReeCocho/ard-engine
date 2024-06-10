use ard_engine::ecs::prelude::*;

#[derive(Resource, Default)]
pub struct SceneGraph {
    roots: Vec<Entity>,
}

impl SceneGraph {
    #[inline]
    pub fn roots(&self) -> &[Entity] {
        &self.roots
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
