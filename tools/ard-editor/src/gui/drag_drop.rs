use crate::assets::meta::MetaFile;

pub enum DragDropPayload {
    Asset(MetaFile),
}
