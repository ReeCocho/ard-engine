use crate::{
    buffer::BufferDescriptor,
    graph::{RenderGraphBuildState, RenderGraphResources},
    image::ImageDescriptor,
    pass::{Pass, PassDescriptor},
};

pub trait Context: Sized {
    type State;
    type Buffer;
    type Image;
    type ImageFormat;
    type CommandBuffer;
    type Pass: Pass<Self>;

    fn create_buffer(
        &mut self,
        descriptor: &BufferDescriptor,
        resources: &RenderGraphResources<Self>,
    ) -> Self::Buffer;

    fn create_image(
        &mut self,
        descriptor: &ImageDescriptor<Self>,
        resources: &RenderGraphResources<Self>,
    ) -> Self::Image;

    fn create_pass(
        &mut self,
        descriptor: &PassDescriptor<Self>,
        state: &RenderGraphBuildState,
        resources: &RenderGraphResources<Self>,
    ) -> Self::Pass;
}
