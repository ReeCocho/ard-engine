use super::ForwardPlus;
use crate::{camera::graph::RenderGraphContext, renderer::graph::GraphBuffer};
use ard_ecs::prelude::*;
use ard_graphics_api::prelude::*;
use ard_render_graph::{
    buffer::BufferId,
    graph::RenderGraphBuilder,
    image::{ImageId, SizeGroupId},
};

use crate::VkBackend;

/// Used by the Forward+ renderer to draw objects.
pub(crate) struct RenderData {
    dynamic_geo_query: Option<ComponentQuery<(Read<Renderable<VkBackend>>, Read<Model>)>>,
    /// Size group for the image drawn to by this renderer.
    size_group: SizeGroupId,
    /// Depth image used for high-z culling.
    highz_image: ImageId,
    /// Buffer that contains all draw calls.
    draw_calls: BufferId,
    /// Previous frames draw call buffer to use when generating the highz image.
    last_draw_calls: GraphBuffer,
    /// Buffer that contains the IDs of objects to render using this renderer.
    input_ids: BufferId,
    /// Output buffer containing IDs of unculled objects.
    output_ids: BufferId,
}

impl RenderData {
    pub(crate) fn new(builder: &mut RenderGraphBuilder<RenderGraphContext<ForwardPlus>>) -> Self {
        todo!()
    }
}
