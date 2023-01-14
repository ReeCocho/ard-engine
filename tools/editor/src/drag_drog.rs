use ard_engine::assets::prelude::AssetNameBuf;

#[derive(Default)]
pub struct DragDrop {
    dropped: bool,
    dragging: bool,
    data: Option<DragDropData>,
}

#[derive(Clone)]
pub enum DragDropData {
    Asset(AssetNameBuf),
}

impl DragDrop {
    pub fn set_drag_state(&mut self, dragging: bool) {
        self.dropped = !dragging && self.dragging;
        self.dragging = dragging;
    }

    pub fn reset_on_drop(&mut self) {
        if self.dropped {
            self.data = None;
        }
    }

    pub fn set_data(&mut self, data: DragDropData) {
        self.data = Some(data);
    }

    pub fn recv(&mut self) -> Option<DragDropData> {
        if self.dropped {
            self.data.take()
        } else {
            None
        }
    }
}
