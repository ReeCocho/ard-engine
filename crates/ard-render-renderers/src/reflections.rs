use ard_pal::prelude::*;
use ard_render_base::{ecs::Frame, resource::ResourceAllocator};
use ard_render_camera::ubo::CameraUbo;
use ard_render_material::{
    binding_table::BindingTableOffset, material::MaterialResource, shader::ShaderResource,
};
use ard_render_raytracing::pipeline::{
    RayTracingMaterialPipeline, RayTracingMaterialPipelineCreateInfo,
};
use ard_render_si::bindings::*;

use crate::passes::RT_PASS_ID;

pub struct Reflections {
    pipeline: RayTracingMaterialPipeline,
    image: Texture,
    sets: Vec<DescriptorSet>,
}

impl Reflections {
    pub fn new<const FIF: usize>(
        ctx: &Context,
        layouts: &Layouts,
        materials: &ResourceAllocator<MaterialResource, FIF>,
        shaders: &ResourceAllocator<ShaderResource, FIF>,
        offset: &BindingTableOffset,
        dims: (u32, u32),
    ) -> Self {
        let image = Self::create_texture(ctx, dims);

        let sets = (0..FIF)
            .map(|_| {
                DescriptorSet::new(
                    ctx.clone(),
                    DescriptorSetCreateInfo {
                        layout: layouts.rt_test.clone(),
                        debug_name: Some("rt_set".into()),
                    },
                )
                .unwrap()
            })
            .collect();

        let raygen = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./rt_test.rgen.spv")),
                debug_name: Some("rt_test_raygen".into()),
            },
        )
        .unwrap();

        let miss = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./rt_test.rmiss.spv")),
                debug_name: Some("rt_test_miss".into()),
            },
        )
        .unwrap();

        Self {
            pipeline: RayTracingMaterialPipeline::new(
                ctx,
                RayTracingMaterialPipelineCreateInfo {
                    pass: RT_PASS_ID,
                    layouts: vec![layouts.camera.clone(), layouts.rt_test.clone()],
                    materials,
                    shaders,
                    offset,
                    raygen,
                    miss,
                    recursion_depth: 1,
                    push_constants: None,
                    debug_name: Some("rt_pipeline".into()),
                },
            ),
            image,
            sets,
        }
    }

    pub fn resize(&mut self, ctx: &Context, dims: (u32, u32)) {
        self.image = Self::create_texture(ctx, dims);
    }

    pub fn update_set(&mut self, frame: Frame, tlas: &TopLevelAccelerationStructure) {
        self.sets[usize::from(frame)].update(&[
            DescriptorSetUpdate {
                binding: RT_TEST_SET_TLAS_BINDING,
                array_element: 0,
                value: DescriptorValue::TopLevelAccelerationStructure(tlas),
            },
            DescriptorSetUpdate {
                binding: RT_TEST_SET_OUTPUT_TEX_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageImage {
                    texture: &self.image,
                    array_element: 0,
                    mip: 0,
                },
            },
        ]);
    }

    pub fn check_for_rebuild<const FIF: usize>(
        &mut self,
        ctx: &Context,
        offset: &BindingTableOffset,
        materials: &ResourceAllocator<MaterialResource, FIF>,
        shaders: &ResourceAllocator<ShaderResource, FIF>,
    ) {
        self.pipeline
            .check_for_rebuild(ctx, offset, materials, shaders);
    }

    pub fn trace<'a>(
        &'a self,
        frame: Frame,
        commands: &mut CommandBuffer<'a>,
        camera: &'a CameraUbo,
    ) {
        commands.ray_trace_pass(self.pipeline.pipeline(), Some("ray_trace"), |pass| {
            pass.bind_sets(
                0,
                vec![camera.get_set(frame), &self.sets[usize::from(frame)]],
            );

            RayTracingDispatch {
                dims: self.image.dims(),
                shader_binding_table: self.pipeline.sbt(),
                raygen_offset: self.pipeline.raygen_offset(),
                miss_offset: self.pipeline.miss_offset(),
                hit_range: self.pipeline.hit_range(),
            }
        });
    }

    fn create_texture(ctx: &Context, dims: (u32, u32)) -> Texture {
        Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: Format::Rgba8Unorm,
                ty: TextureType::Type2D,
                width: dims.0,
                height: dims.1,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                sample_count: MultiSamples::Count1,
                texture_usage: TextureUsage::STORAGE,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("rt_tex".into()),
            },
        )
        .unwrap()
    }
}
