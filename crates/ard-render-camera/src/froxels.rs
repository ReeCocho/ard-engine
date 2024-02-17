use ard_pal::prelude::{
    CommandBuffer, ComputePass, ComputePipeline, ComputePipelineCreateInfo, Context, QueueType,
    Shader, ShaderCreateInfo,
};
use ard_render_base::ecs::Frame;
use ard_render_si::{
    bindings::Layouts,
    consts::{CAMERA_FROXELS_HEIGHT, CAMERA_FROXELS_WIDTH},
};

use crate::ubo::CameraUbo;

pub struct FroxelGenPipeline {
    pipeline: ComputePipeline,
}

impl FroxelGenPipeline {
    pub fn new(ctx: &Context, layouts: &Layouts) -> Self {
        let module = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./froxel_gen.comp.spv")),
                debug_name: Some("froxel_gen_shader".into()),
            },
        )
        .unwrap();

        let pipeline = ComputePipeline::new(
            ctx.clone(),
            ComputePipelineCreateInfo {
                layouts: vec![layouts.froxel_gen.clone()],
                module,
                work_group_size: (CAMERA_FROXELS_WIDTH as u32, CAMERA_FROXELS_HEIGHT as u32, 1),
                push_constants_size: None,
                debug_name: Some("froxel_gen_pipeline".into()),
            },
        )
        .unwrap();

        Self { pipeline }
    }

    pub fn regen<'a>(&self, frame: Frame, commands: &mut CommandBuffer<'a>, camera: &'a CameraUbo) {
        commands.compute_pass(&self.pipeline, Some("froxel_gen"), |pass| {
            pass.bind_sets(0, vec![camera.froxel_regen_set(frame)]);
            (1, 1, 1)
        });
    }
}
