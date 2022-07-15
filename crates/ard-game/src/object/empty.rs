use serde::{Deserialize, Serialize};

use crate::components::transform::{Children, Parent, Transform};
use crate::game_object_def;
use ard_graphics_api::prelude::Model;

game_object_def!(
    EmptyObject,
    Transform
    Parent
    Children
    Model
);
