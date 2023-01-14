use ard_assets::{
    manager::Assets,
    prelude::{Asset, AssetNameBuf, Handle},
};
use ard_ecs::prelude::Entity;
use ard_math::{Mat4, Quat, Vec2, Vec3, Vec3A, Vec4};
use ard_render::renderer::Model;
use smallvec::SmallVec;

use crate::scene::{EntityMap, MappedEntity};

pub trait SaveLoad: Sized {
    type Descriptor: Default;

    fn save(&self, entities: &EntityMap, assets: &Assets) -> Self::Descriptor;

    fn load(descriptor: Self::Descriptor, entities: &EntityMap, assets: &Assets) -> Self;
}

macro_rules! private_save_load_impl {
    ($ty:ident) => {
        impl SaveLoad for $ty {
            type Descriptor = $ty;

            #[inline]
            fn save(&self, _entities: &EntityMap, _assets: &Assets) -> Self::Descriptor {
                self.clone()
            }

            #[inline]
            fn load(descriptor: Self::Descriptor, _entities: &EntityMap, _assets: &Assets) -> Self {
                descriptor
            }
        }
    };
}

macro_rules! private_save_load_arr_impl {
    ($n:literal) => {
        impl<T: SaveLoad> SaveLoad for SmallVec<[T; $n]> {
            type Descriptor = SmallVec<[T::Descriptor; $n]>;

            #[inline]
            fn save(&self, entities: &EntityMap, assets: &Assets) -> Self::Descriptor {
                let mut descriptor = Self::Descriptor::with_capacity(self.len());
                for elem in self {
                    descriptor.push(elem.save(entities, assets));
                }
                descriptor
            }

            #[inline]
            fn load(descriptor: Self::Descriptor, entities: &EntityMap, assets: &Assets) -> Self {
                let mut ret = Self::with_capacity(descriptor.len());
                for elem in descriptor {
                    ret.push(T::load(elem, entities, assets));
                }
                ret
            }
        }
    };
}

impl SaveLoad for Entity {
    type Descriptor = MappedEntity;

    fn save(&self, entities: &EntityMap, _assets: &Assets) -> Self::Descriptor {
        entities.to_map(*self)
    }

    fn load(descriptor: Self::Descriptor, entities: &EntityMap, _assets: &Assets) -> Self {
        entities.from_map(descriptor)
    }
}

impl<A: Asset + 'static> SaveLoad for Handle<A> {
    type Descriptor = AssetNameBuf;

    fn save(&self, _entities: &EntityMap, assets: &Assets) -> Self::Descriptor {
        assets.get_name(self)
    }

    fn load(descriptor: Self::Descriptor, _entities: &EntityMap, assets: &Assets) -> Self {
        assets.load(&descriptor)
    }
}

impl SaveLoad for Model {
    type Descriptor = Model;

    fn save(&self, _entities: &EntityMap, _assets: &Assets) -> Self::Descriptor {
        self.clone()
    }

    fn load(descriptor: Self::Descriptor, _entities: &EntityMap, _assets: &Assets) -> Self {
        descriptor
    }
}

impl<T: SaveLoad> SaveLoad for Option<T> {
    type Descriptor = Option<T::Descriptor>;

    fn save(&self, entities: &EntityMap, assets: &Assets) -> Self::Descriptor {
        match self {
            Some(inner) => Some(inner.save(entities, assets)),
            None => None,
        }
    }

    fn load(descriptor: Self::Descriptor, entities: &EntityMap, assets: &Assets) -> Self {
        match descriptor {
            Some(inner) => Some(T::load(inner, entities, assets)),
            None => None,
        }
    }
}

private_save_load_impl!(bool);
private_save_load_impl!(char);
private_save_load_impl!(f32);
private_save_load_impl!(f64);
private_save_load_impl!(usize);
private_save_load_impl!(u8);
private_save_load_impl!(u16);
private_save_load_impl!(u32);
private_save_load_impl!(u64);
private_save_load_impl!(isize);
private_save_load_impl!(i8);
private_save_load_impl!(i16);
private_save_load_impl!(i32);
private_save_load_impl!(i64);
private_save_load_impl!(String);
private_save_load_impl!(Vec2);
private_save_load_impl!(Vec3);
private_save_load_impl!(Vec3A);
private_save_load_impl!(Vec4);
private_save_load_impl!(Mat4);
private_save_load_impl!(Quat);

private_save_load_arr_impl!(0);
private_save_load_arr_impl!(1);
private_save_load_arr_impl!(2);
private_save_load_arr_impl!(3);
private_save_load_arr_impl!(4);
private_save_load_arr_impl!(5);
private_save_load_arr_impl!(6);
private_save_load_arr_impl!(7);
private_save_load_arr_impl!(8);
private_save_load_arr_impl!(9);
private_save_load_arr_impl!(10);
private_save_load_arr_impl!(11);
private_save_load_arr_impl!(12);
private_save_load_arr_impl!(13);
private_save_load_arr_impl!(14);
private_save_load_arr_impl!(15);
private_save_load_arr_impl!(16);
private_save_load_arr_impl!(17);
private_save_load_arr_impl!(18);
private_save_load_arr_impl!(19);
private_save_load_arr_impl!(20);
private_save_load_arr_impl!(21);
private_save_load_arr_impl!(22);
private_save_load_arr_impl!(23);
private_save_load_arr_impl!(24);
private_save_load_arr_impl!(25);
private_save_load_arr_impl!(26);
private_save_load_arr_impl!(27);
private_save_load_arr_impl!(28);
private_save_load_arr_impl!(29);
private_save_load_arr_impl!(30);
private_save_load_arr_impl!(31);
private_save_load_arr_impl!(32);
