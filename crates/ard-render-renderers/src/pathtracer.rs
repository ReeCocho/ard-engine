use ard_ecs::resource::Resource;
use ard_pal::prelude::*;
use ard_render_base::{ecs::Frame, resource::ResourceAllocator, FRAMES_IN_FLIGHT};
use ard_render_camera::ubo::CameraUbo;
use ard_render_material::{factory::MaterialFactory, material::MaterialResource};
use ard_render_meshes::factory::MeshFactory;
use ard_render_objects::objects::RenderObjects;
use ard_render_raytracing::pipeline::{
    RayTracingMaterialPipeline, RayTracingMaterialPipelineCreateInfo,
};
use ard_render_si::{bindings::*, types::*};
use ard_render_textures::factory::TextureFactory;

use crate::passes::{pathtracer::PathTracerPassSets, PATH_TRACER_PASS_ID};

const MAX_SAMPLES: u32 = 2048;

#[derive(Copy, Clone, Resource, Default)]
pub struct PathTracerSettings {
    pub enabled: bool,
}

pub struct PathTracer {
    pipeline: RayTracingMaterialPipeline,
    image: Texture,
    sets: PathTracerPassSets,
    current_sample: Option<u32>,
}

impl PathTracer {
    pub fn new(
        ctx: &Context,
        layouts: &Layouts,
        materials: &ResourceAllocator<MaterialResource>,
        factory: &MaterialFactory,
        dims: (u32, u32),
    ) -> Self {
        let image = Self::create_texture(ctx, dims);
        let mut sets = PathTracerPassSets::new(ctx, layouts);

        for i in 0..FRAMES_IN_FLIGHT {
            sets.update_output(Frame::from(i), &image);
        }

        let raygen = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./pathtracer.rgen.spv")),
                debug_name: Some("path_tracer_raygen".into()),
            },
        )
        .unwrap();

        let miss = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./pathtracer.rmiss.spv")),
                debug_name: Some("path_tracer_miss".into()),
            },
        )
        .unwrap();

        Self {
            pipeline: RayTracingMaterialPipeline::new(
                ctx,
                RayTracingMaterialPipelineCreateInfo {
                    pass: PATH_TRACER_PASS_ID,
                    layouts: vec![
                        layouts.path_tracer_pass.clone(),
                        layouts.camera.clone(),
                        layouts.mesh_data.clone(),
                        layouts.texture_slots.clone(),
                        layouts.textures.clone(),
                    ],
                    materials,
                    factory,
                    raygen,
                    miss,
                    debug_name: Some("path_tracer_pipeline".into()),
                },
            ),
            image,
            sets,
            current_sample: None,
        }
    }

    #[inline(always)]
    pub fn image(&self) -> &Texture {
        &self.image
    }

    #[inline(always)]
    pub fn sets(&mut self) -> &mut PathTracerPassSets {
        &mut self.sets
    }

    pub fn resize(&mut self, ctx: &Context, dims: (u32, u32)) {
        self.image = Self::create_texture(ctx, dims);
    }

    pub fn check_for_rebuild(
        &mut self,
        ctx: &Context,
        materials: &ResourceAllocator<MaterialResource>,
        factory: &MaterialFactory,
    ) {
        self.pipeline.check_for_rebuild(ctx, materials, factory);
    }

    pub fn update_bindings(
        &mut self,
        frame: Frame,
        tlas: &TopLevelAccelerationStructure,
        objects: &RenderObjects,
    ) {
        self.sets
            .update_object_data_bindings(frame, objects.object_data());
        self.sets.update_tlas(frame, tlas);
        self.sets.update_output(frame, &self.image);
    }

    pub fn update_settings(&mut self, settings: &PathTracerSettings) {
        if !settings.enabled {
            self.current_sample = None;
            return;
        }

        self.current_sample = match self.current_sample {
            Some(old) => Some(old + 1),
            None => Some(0),
        };
    }

    pub fn trace<'a>(
        &'a self,
        frame: Frame,
        commands: &mut CommandBuffer<'a>,
        camera: &'a CameraUbo,
        mesh_factory: &'a MeshFactory,
        material_factory: &'a MaterialFactory,
        texture_factory: &'a TextureFactory,
    ) {
        let sample = match self.current_sample {
            Some(sample) => sample,
            None => return,
        };

        if sample >= MAX_SAMPLES {
            return;
        }

        commands.ray_trace_pass(self.pipeline.pipeline(), Some("ray_trace"), |pass| {
            pass.bind_sets(
                0,
                vec![
                    &self.sets.get_set(frame),
                    camera.get_set(frame),
                    mesh_factory.mesh_data_set(frame),
                    material_factory.get_texture_slots_set(frame),
                ],
            );

            unsafe {
                pass.bind_sets_unchecked(4, vec![texture_factory.get_set(frame)]);
            }

            let consts = [GpuPathTracerPushConstants {
                sample_batch: self.current_sample.unwrap(),
            }];
            pass.push_constants(bytemuck::cast_slice(&consts));

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
                texture_usage: TextureUsage::STORAGE | TextureUsage::SAMPLED,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("pathtracer_tex".into()),
            },
        )
        .unwrap()
    }
}
