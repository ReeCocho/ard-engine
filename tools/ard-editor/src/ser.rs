use ard_engine::{
    core::{core::Name, destroy::Destroy, stat::Static},
    game::components::stat::MarkStatic,
    physics::{
        collider::{Collider, ColliderHandle},
        rigid_body::{RigidBody, RigidBodyHandle},
    },
    render::{
        loader::{MaterialHandle, MeshHandle},
        prelude::RenderingMode,
        MaterialInstance, Mesh, RenderFlags,
    },
    save_load::{format::SaveFormat, load_data::Loader, save_data::Saver},
    transform::{Children, Model, Parent, Position, Rotation, Scale, SetParent},
};

use crate::inspect::transform::EulerRotation;

pub fn saver<F: SaveFormat + 'static>() -> Saver<F> {
    Saver::default()
        .include_component::<Position>()
        .include_component::<Rotation>()
        .include_component::<Scale>()
        .include_component::<Parent>()
        .include_component::<Children>()
        .include_component::<Model>()
        .include_component::<RenderingMode>()
        .include_component::<RenderFlags>()
        .include_component::<MeshHandle>()
        .include_component::<MaterialHandle>()
        .include_component::<Name>()
        .include_component::<MarkStatic>()
        .include_component::<Collider>()
        .include_component::<RigidBody>()
        .ignore::<ColliderHandle>()
        .ignore::<RigidBodyHandle>()
        .ignore::<Static>()
        .ignore::<Mesh>()
        .ignore::<MaterialInstance>()
        .ignore::<Destroy>()
        .ignore::<SetParent>()
        .ignore::<EulerRotation>()
}

pub fn loader<F: SaveFormat + 'static>() -> Loader<F> {
    Loader::default()
        .load_component::<Position>()
        .load_component::<Rotation>()
        .load_component::<Scale>()
        .load_component::<Parent>()
        .load_component::<Children>()
        .load_component::<Model>()
        .load_component::<RenderingMode>()
        .load_component::<RenderFlags>()
        .load_component::<MeshHandle>()
        .load_component::<MaterialHandle>()
        .load_component::<Name>()
        .load_component::<MarkStatic>()
        .load_component::<Collider>()
        .load_component::<RigidBody>()
}
