use std::any::TypeId;

use crate::{LoadContext, SaveLoad};
use ard_ecs::{
    archetype::{storage::AnyArchetypeStorage, Archetypes},
    prelude::*,
    tag::storage::TagStorage,
};

pub struct ComponentLoader<C: SaveLoad> {
    to_load: Vec<C>,
}

pub struct TagLoader<T: SaveLoad> {
    to_load: Vec<T>,
}

impl<C: SaveLoad> Default for ComponentLoader<C> {
    fn default() -> Self {
        Self {
            to_load: Vec::default(),
        }
    }
}

impl<C: SaveLoad> Default for TagLoader<C> {
    fn default() -> Self {
        Self {
            to_load: Vec::default(),
        }
    }
}

pub trait GenericComponentLoader: Send + Sync {
    fn type_id(&self) -> TypeId;

    fn new_storage(&self, archetypes: &mut Archetypes) -> usize;

    fn deserialize_all(&mut self, ctx: &LoadContext, raw: Vec<u8>);

    fn move_into(&mut self, archetypes: &Archetypes, idx: usize);
}

pub trait GenericTagLoader: Send + Sync {
    fn type_id(&self) -> TypeId;

    fn deserialize_all(&mut self, ctx: &LoadContext, raw: Vec<u8>);

    fn move_into(&mut self, tags: &mut Tags, entities: &[Entity]);
}

impl<C: Component + SaveLoad + 'static> GenericComponentLoader for ComponentLoader<C> {
    fn type_id(&self) -> TypeId {
        TypeId::of::<C>()
    }

    fn deserialize_all(&mut self, ctx: &LoadContext, raw: Vec<u8>) {
        let intermediates = bincode::deserialize::<Vec<C::Intermediate>>(&raw).unwrap();
        self.to_load = intermediates.into_iter().map(|i| C::load(ctx, i)).collect();
    }

    fn move_into(&mut self, archetypes: &Archetypes, idx: usize) {
        let mut buffer = archetypes.get_storage::<C>().unwrap().get_mut(idx);
        buffer.extend(std::mem::take(&mut self.to_load).into_iter());
    }

    fn new_storage(&self, archetypes: &mut Archetypes) -> usize {
        match archetypes.get_storage_mut::<C>() {
            Some(storage) => storage,
            None => {
                archetypes.create_storage::<C>();
                archetypes.get_storage_mut::<C>().unwrap()
            }
        }
        .create()
    }
}

impl<T: Tag + SaveLoad + 'static> GenericTagLoader for TagLoader<T> {
    fn type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }

    fn deserialize_all(&mut self, ctx: &LoadContext, raw: Vec<u8>) {
        let intermediates = bincode::deserialize::<Vec<T::Intermediate>>(&raw).unwrap();
        self.to_load = intermediates.into_iter().map(|i| T::load(ctx, i)).collect();
    }

    fn move_into(&mut self, tags: &mut Tags, entities: &[Entity]) {
        let mut storage = tags.get_storage_or_default_mut::<T>();
        for (i, tag) in self.to_load.drain(..).enumerate() {
            storage.add(entities[i], tag);
        }
    }
}
