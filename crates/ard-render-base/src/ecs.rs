/// Frame index.
#[derive(Debug, Copy, Clone)]
pub struct Frame(usize);

impl From<usize> for Frame {
    fn from(value: usize) -> Self {
        Frame(value)
    }
}

impl From<Frame> for usize {
    fn from(value: Frame) -> Self {
        value.0
    }
}
