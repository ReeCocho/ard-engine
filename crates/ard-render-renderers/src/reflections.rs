use ard_pal::prelude::*;
use ard_render_base::{ecs::Frame, resource::ResourceAllocator};
use ard_render_camera::ubo::CameraUbo;
use ard_render_material::{factory::MaterialFactory, material::MaterialResource};
use ard_render_raytracing::pipeline::{
    RayTracingMaterialPipeline, RayTracingMaterialPipelineCreateInfo,
};
use ard_render_si::bindings::*;

use crate::passes::{reflections::ReflectionPassSets, RT_PASS_ID};

pub struct Reflections {
    pipeline: RayTracingMaterialPipeline,
    image: Texture,
    sets: ReflectionPassSets,
}

impl Reflections {
    pub fn new<const FIF: usize>(
        ctx: &Context,
        layouts: &Layouts,
        materials: &ResourceAllocator<MaterialResource, FIF>,
        factory: &MaterialFactory<FIF>,
        dims: (u32, u32),
    ) -> Self {
        let image = Self::create_texture(ctx, dims);
        let mut sets = ReflectionPassSets::new(ctx, layouts, FIF);

        for i in 0..FIF {
            sets.update_output(Frame::from(i), &image);
        }

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
                    layouts: vec![layouts.camera.clone(), layouts.reflection_rt_pass.clone()],
                    materials,
                    factory,
                    raygen,
                    miss,
                    debug_name: Some("rt_pipeline".into()),
                },
            ),
            image,
            sets,
        }
    }

    #[inline(always)]
    pub fn sets(&mut self) -> &mut ReflectionPassSets {
        &mut self.sets
    }

    pub fn resize(&mut self, ctx: &Context, dims: (u32, u32)) {
        self.image = Self::create_texture(ctx, dims);
    }

    pub fn update_bindings(&mut self, frame: Frame, tlas: &TopLevelAccelerationStructure) {
        self.sets.update_tlas(frame, tlas);
        self.sets.update_output(frame, &self.image);
    }

    pub fn check_for_rebuild<const FIF: usize>(
        &mut self,
        ctx: &Context,
        materials: &ResourceAllocator<MaterialResource, FIF>,
        factory: &MaterialFactory<FIF>,
    ) {
        self.pipeline.check_for_rebuild(ctx, materials, factory);
    }

    pub fn trace<'a>(
        &'a self,
        frame: Frame,
        commands: &mut CommandBuffer<'a>,
        camera: &'a CameraUbo,
    ) {
        commands.ray_trace_pass(self.pipeline.pipeline(), Some("ray_trace"), |pass| {
            pass.bind_sets(0, vec![camera.get_set(frame), &self.sets.get_set(frame)]);

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
                format: Format::Rgba16SFloat,
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
