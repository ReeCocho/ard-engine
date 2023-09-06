use ard_pal::prelude::*;

pub trait RenderModule {
    fn initialize(ctx: Context) -> Self;
}
