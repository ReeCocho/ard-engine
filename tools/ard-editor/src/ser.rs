use ard_engine::{
    game::save_data::SceneAsset,
    save_load::{format::SaveFormat, load_data::Loader, save_data::Saver},
};

use crate::inspect::transform::EulerRotation;

pub fn saver<F: SaveFormat + 'static>() -> Saver<F> {
    SceneAsset::saver().ignore::<EulerRotation>()
}

pub fn loader<F: SaveFormat + 'static>() -> Loader<F> {
    SceneAsset::loader()
}
