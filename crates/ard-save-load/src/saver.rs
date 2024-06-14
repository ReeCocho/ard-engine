use crate::{SaveContext, SaveLoad};
use ard_ecs::prelude::*;

pub struct ComponentSaver<C: SaveLoad> {
    to_save: Vec<C::Intermediate>,
}

impl<C: SaveLoad> Default for ComponentSaver<C> {
    fn default() -> Self {
        Self {
            to_save: Vec::default(),
        }
    }
}

pub struct TagSaver<T: SaveLoad> {
    to_save: Vec<T::Intermediate>,
}

impl<T: SaveLoad> Default for TagSaver<T> {
    fn default() -> Self {
        Self {
            to_save: Vec::default(),
        }
    }
}

pub trait GenericSaver {
    fn add(&mut self, ctx: &SaveContext, entity: Entity, queries: &Queries<Everything>);

    fn serialize_all(&mut self) -> Vec<u8>;
}

impl<C: Component + SaveLoad + 'static> GenericSaver for ComponentSaver<C> {
    fn add(&mut self, ctx: &SaveContext, entity: Entity, queries: &Queries<Everything>) {
        let component = queries.get::<Read<C>>(entity).unwrap();
        self.to_save.push(component.save(ctx));
    }

    fn serialize_all(&mut self) -> Vec<u8> {
        let res = bincode::serialize(&self.to_save).unwrap();
        self.to_save.clear();
        res
    }
}

impl<T: Tag + SaveLoad + 'static> GenericSaver for TagSaver<T> {
    fn add(&mut self, ctx: &SaveContext, entity: Entity, queries: &Queries<Everything>) {
        let tag = queries.get_tag::<Read<T>>(entity).unwrap();
        self.to_save.push(tag.save(ctx));
    }

    fn serialize_all(&mut self) -> Vec<u8> {
        let res = bincode::serialize(&self.to_save).unwrap();
        self.to_save.clear();
        res
    }
}
