pub mod buffer;
pub mod context;
pub mod graph;
pub mod image;
pub mod pass;

#[derive(Debug, Copy, Clone)]
pub enum LoadOp<V: Copy + Clone> {
    Clear(V),
    Load,
    DontCare,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AccessType {
    Read,
    ReadWrite,
}

#[derive(Debug, Copy, Clone)]
pub struct Operations<V: Copy + Clone> {
    pub load: LoadOp<V>,
    pub store: bool,
}

#[cfg(test)]
mod tests {
    use crate::{
        buffer::{BufferDescriptor, BufferUsage},
        context::Context,
        graph::{GraphBuildError, RenderGraphBuildState, RenderGraphBuilder, RenderGraphResources},
        image::{ImageDescriptor, SizeGroup},
        pass::{ColorAttachmentDescriptor, DepthStencilAttachmentDescriptor, Pass, PassDescriptor},
        LoadOp, Operations,
    };

    #[derive(Debug)]
    struct DummyContext;

    #[derive(Debug)]
    struct DummyPass;

    impl Pass<DummyContext> for DummyPass {
        fn run(
            &mut self,
            _command_buffer: &<DummyContext as Context>::CommandBuffer,
            _ctx: &mut DummyContext,
            _state: &mut (),
            _resources: &mut RenderGraphResources<DummyContext>,
        ) {
        }
    }

    impl Context for DummyContext {
        type State = ();
        type Buffer = ();
        type Image = ();
        type ImageFormat = ();
        type CommandBuffer = ();
        type Pass = DummyPass;

        fn create_buffer(
            &mut self,
            _descriptor: &BufferDescriptor,
            _resources: &RenderGraphResources<Self>,
        ) -> Self::Buffer {
            ()
        }

        fn create_image(
            &mut self,
            _descriptor: &ImageDescriptor<Self>,
            _resources: &RenderGraphResources<Self>,
        ) -> Self::Image {
            ()
        }

        fn create_pass(
            &mut self,
            _descriptor: &PassDescriptor<Self>,
            _state: &RenderGraphBuildState,
            _resources: &RenderGraphResources<Self>,
        ) -> Self::Pass {
            DummyPass
        }
    }

    #[test]
    fn example_api() {
        let mut graph_builder = RenderGraphBuilder::new();

        let _cpu_pass = graph_builder.add_pass(PassDescriptor::ComputePass {
            toggleable: false,
            images: Vec::default(),
            buffers: Vec::default(),
            code: |_, _, _, _, _| {},
        });

        let _gpu_pass = graph_builder.add_pass(PassDescriptor::RenderPass {
            toggleable: false,
            color_attachments: Vec::default(),
            depth_stencil_attachment: None,
            buffers: Vec::default(),
            code: |_, _, _, _, _| {},
        });

        let _compute_pass = graph_builder.add_pass(PassDescriptor::ComputePass {
            toggleable: false,
            images: Vec::default(),
            buffers: Vec::default(),
            code: |_, _, _, _, _| {},
        });

        let mut ctx = DummyContext;
        let _graph = graph_builder.build(&mut ctx);
    }

    #[test]
    fn size_groups() {
        let mut ctx = DummyContext;

        let mut graph_builder = RenderGraphBuilder::new();
        let bad_width = graph_builder.add_size_group(SizeGroup {
            width: 0,
            height: 1,
            mip_levels: 1,
            array_layers: 1,
        });
        assert_eq!(
            GraphBuildError::InvalidSizeGroup(bad_width),
            graph_builder.build(&mut ctx).unwrap_err()
        );

        let mut graph_builder = RenderGraphBuilder::new();
        let bad_height = graph_builder.add_size_group(SizeGroup {
            width: 1,
            height: 0,
            mip_levels: 1,
            array_layers: 1,
        });
        assert_eq!(
            GraphBuildError::InvalidSizeGroup(bad_height),
            graph_builder.build(&mut ctx).unwrap_err()
        );

        let mut graph_builder = RenderGraphBuilder::new();
        let bad_mip = graph_builder.add_size_group(SizeGroup {
            width: 1,
            height: 1,
            mip_levels: 0,
            array_layers: 1,
        });
        assert_eq!(
            GraphBuildError::InvalidSizeGroup(bad_mip),
            graph_builder.build(&mut ctx).unwrap_err()
        );

        let mut graph_builder = RenderGraphBuilder::new();
        let bad_layers = graph_builder.add_size_group(SizeGroup {
            width: 1,
            height: 1,
            mip_levels: 1,
            array_layers: 0,
        });
        assert_eq!(
            GraphBuildError::InvalidSizeGroup(bad_layers),
            graph_builder.build(&mut ctx).unwrap_err()
        );

        let mut graph_builder = RenderGraphBuilder::new();
        let _ = graph_builder.add_size_group(SizeGroup {
            width: 1,
            height: 1,
            mip_levels: 1,
            array_layers: 1,
        });
        graph_builder.build(&mut ctx).unwrap();
    }

    #[test]
    fn buffers() {
        let mut ctx = DummyContext;

        let mut graph_builder = RenderGraphBuilder::new();
        let bad = graph_builder.add_buffer(BufferDescriptor {
            size: 0,
            usage: BufferUsage::UniformBuffer,
        });
        assert_eq!(
            GraphBuildError::InvalidBufferSize(bad),
            graph_builder.build(&mut ctx).unwrap_err()
        );

        let mut graph_builder = RenderGraphBuilder::new();
        let _ = graph_builder.add_buffer(BufferDescriptor {
            size: 1,
            usage: BufferUsage::UniformBuffer,
        });
        graph_builder.build(&mut ctx).unwrap();
    }

    #[test]
    fn attachments() {
        let mut ctx = DummyContext;

        let mut graph_builder = RenderGraphBuilder::new();
        let bad = graph_builder.add_pass(PassDescriptor::RenderPass {
            toggleable: false,
            color_attachments: Vec::default(),
            depth_stencil_attachment: None,
            buffers: Vec::default(),
            code: |_, _, _, _, _| {},
        });
        assert_eq!(
            GraphBuildError::NoAttachments(bad),
            graph_builder.build(&mut ctx).unwrap_err()
        );

        let mut graph_builder = RenderGraphBuilder::new();
        let size_group = graph_builder.add_size_group(SizeGroup {
            width: 1,
            height: 1,
            mip_levels: 1,
            array_layers: 1,
        });

        let image = graph_builder.add_image(ImageDescriptor {
            size_group,
            format: (),
        });

        let _ = graph_builder.add_pass(PassDescriptor::RenderPass {
            toggleable: false,
            color_attachments: Vec::default(),
            depth_stencil_attachment: Some(DepthStencilAttachmentDescriptor {
                image,
                ops: Operations {
                    load: LoadOp::DontCare,
                    store: false,
                },
            }),
            buffers: Vec::default(),
            code: |_, _, _, _, _| {},
        });

        let _ = graph_builder.add_pass(PassDescriptor::RenderPass {
            toggleable: false,
            color_attachments: vec![ColorAttachmentDescriptor {
                image,
                ops: Operations {
                    load: LoadOp::DontCare,
                    store: false,
                },
            }],
            depth_stencil_attachment: None,
            buffers: Vec::default(),
            code: |_, _, _, _, _| {},
        });

        graph_builder.build(&mut ctx).unwrap();
    }

    #[test]
    fn mixed_size_groups() {
        let mut ctx = DummyContext;

        let mut graph_builder = RenderGraphBuilder::new();

        let size_group1 = graph_builder.add_size_group(SizeGroup {
            width: 1,
            height: 1,
            mip_levels: 1,
            array_layers: 1,
        });

        let size_group2 = graph_builder.add_size_group(SizeGroup {
            width: 1,
            height: 1,
            mip_levels: 1,
            array_layers: 1,
        });

        let image1 = graph_builder.add_image(ImageDescriptor {
            size_group: size_group1,
            format: (),
        });

        let image2 = graph_builder.add_image(ImageDescriptor {
            size_group: size_group2,
            format: (),
        });

        let bad = graph_builder.add_pass(PassDescriptor::RenderPass {
            toggleable: false,
            color_attachments: vec![
                ColorAttachmentDescriptor {
                    image: image1,
                    ops: Operations {
                        load: LoadOp::DontCare,
                        store: false,
                    },
                },
                ColorAttachmentDescriptor {
                    image: image2,
                    ops: Operations {
                        load: LoadOp::DontCare,
                        store: false,
                    },
                },
            ],
            depth_stencil_attachment: None,
            buffers: Vec::default(),
            code: |_, _, _, _, _| {},
        });

        assert_eq!(
            GraphBuildError::MixedSizeGroups(bad),
            graph_builder.build(&mut ctx).unwrap_err()
        );

        let mut graph_builder = RenderGraphBuilder::new();

        let size_group = graph_builder.add_size_group(SizeGroup {
            width: 1,
            height: 1,
            mip_levels: 1,
            array_layers: 1,
        });

        let image1 = graph_builder.add_image(ImageDescriptor {
            size_group,
            format: (),
        });

        let image2 = graph_builder.add_image(ImageDescriptor {
            size_group,
            format: (),
        });

        let _ = graph_builder.add_pass(PassDescriptor::RenderPass {
            toggleable: false,
            color_attachments: vec![
                ColorAttachmentDescriptor {
                    image: image1,
                    ops: Operations {
                        load: LoadOp::DontCare,
                        store: false,
                    },
                },
                ColorAttachmentDescriptor {
                    image: image2,
                    ops: Operations {
                        load: LoadOp::DontCare,
                        store: false,
                    },
                },
            ],
            depth_stencil_attachment: None,
            buffers: Vec::default(),
            code: |_, _, _, _, _| {},
        });

        graph_builder.build(&mut ctx).unwrap();
    }
}
