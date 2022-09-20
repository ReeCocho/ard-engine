use api::types::QueueType;

pub struct Job {
    pub(crate) ty: QueueType,
    pub(crate) target_value: u64,
}
