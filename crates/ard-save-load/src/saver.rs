use crate::{format::SaveFormat, SaveContext, SaveLoad};
use ard_ecs::prelude::*;

pub struct ComponentSaver<F: SaveFormat, C: SaveLoad> {
    to_save: Vec<C::Intermediate>,
    _format: std::marker::PhantomData<F>,
}

impl<F: SaveFormat, C: SaveLoad> Default for ComponentSaver<F, C> {
    fn default() -> Self {
        Self {
            to_save: Vec::default(),
            _format: Default::default(),
        }
    }
}

pub struct TagSaver<F: SaveFormat, T: SaveLoad> {
    to_save: Vec<T::Intermediate>,
    _format: std::marker::PhantomData<F>,
}

impl<F: SaveFormat, T: SaveLoad> Default for TagSaver<F, T> {
    fn default() -> Self {
        Self {
            to_save: Vec::default(),
            _format: Default::default(),
        }
    }
}

pub trait GenericSaver<F: SaveFormat> {
    fn add(&mut self, ctx: &mut SaveContext, entity: Entity, queries: &Queries<Everything>);

    fn serialize_all(&mut self) -> Result<Vec<u8>, F::SerializeError>;
}

impl<F: SaveFormat, C: Component + SaveLoad + 'static> GenericSaver<F> for ComponentSaver<F, C> {
    fn add(&mut self, ctx: &mut SaveContext, entity: Entity, queries: &Queries<Everything>) {
        let component = queries.get::<Read<C>>(entity).unwrap();
        self.to_save.push(component.save(ctx));
    }

    fn serialize_all(&mut self) -> Result<Vec<u8>, F::SerializeError> {
        let res = F::serialize(&self.to_save)?;
        self.to_save.clear();
        Ok(res)
    }
}

impl<F: SaveFormat, T: Tag + SaveLoad + 'static> GenericSaver<F> for TagSaver<F, T> {
    fn add(&mut self, ctx: &mut SaveContext, entity: Entity, queries: &Queries<Everything>) {
        let tag = queries.get_tag::<Read<T>>(entity).unwrap();
        self.to_save.push(tag.save(ctx));
    }

    fn serialize_all(&mut self) -> Result<Vec<u8>, F::SerializeError> {
        let res = F::serialize(&self.to_save)?;
        self.to_save.clear();
        Ok(res)
    }
}
