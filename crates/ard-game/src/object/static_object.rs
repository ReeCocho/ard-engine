use serde::{Deserialize, Serialize};

use crate::components::transform::{Parent, Transform};
use crate::game_object_def;

game_object_def!(
    StaticObject,
    Transform
    Parent
);
