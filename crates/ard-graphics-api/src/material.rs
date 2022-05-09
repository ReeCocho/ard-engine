use crate::Backend;

pub struct MaterialCreateInfo<B: Backend> {
    pub pipeline: B::Pipeline,
}

pub trait MaterialApi: Clone + Send + Sync {}
