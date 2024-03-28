use std::ops::Range;

use ard_pal::prelude::*;
use ard_render_base::resource::ResourceAllocator;
use ard_render_material::{
    binding_table::BindingTableOffset, factory::PassId, material::MaterialResource,
    shader::ShaderResource,
};

pub struct RayTracingMaterialPipelineCreateInfo<'a, const FIF: usize> {
    pub pass: PassId,
    pub layouts: Vec<DescriptorSetLayout>,
    pub materials: &'a ResourceAllocator<MaterialResource, FIF>,
    pub shaders: &'a ResourceAllocator<ShaderResource, FIF>,
    pub offset: &'a BindingTableOffset,
    pub raygen: Shader,
    pub miss: Shader,
    pub recursion_depth: u32,
    pub push_constants: Option<u32>,
    pub debug_name: Option<String>,
}

pub struct RayTracingMaterialPipeline {
    sbt: Buffer,
    pass: PassId,
    pipeline: RayTracingPipeline,
    layouts: Vec<DescriptorSetLayout>,
    push_constants: Option<u32>,
    debug_name: Option<String>,
    raygen: Shader,
    miss: Shader,
    recursion_depth: u32,
    last_table_size: usize,
    table_bases: TableBases,
}

#[derive(Clone, Copy)]
struct TableBases {
    pub hit_start: u64,
    pub hit_end: u64,
    pub raygen: u64,
    pub miss: u64,
}

impl RayTracingMaterialPipeline {
    pub fn new<const FIF: usize>(
        ctx: &Context,
        create_info: RayTracingMaterialPipelineCreateInfo<FIF>,
    ) -> Self {
        let pass = create_info.pass;
        let layouts = create_info.layouts.clone();
        let push_constants = create_info.push_constants.clone();
        let debug_name = create_info.debug_name.clone();
        let raygen = create_info.raygen.clone();
        let miss = create_info.miss.clone();
        let recursion_depth = create_info.recursion_depth;
        let last_table_size = create_info.offset.binding_table_size();

        let size = create_info.offset.binding_table_size();
        let pipeline = Self::create_pipeline(ctx, create_info);
        let (sbt, table_bases) = Self::create_sbt(ctx, &pipeline, size);

        Self {
            sbt,
            pass,
            pipeline,
            layouts,
            push_constants,
            debug_name,
            raygen,
            miss,
            last_table_size,
            table_bases,
            recursion_depth,
        }
    }

    #[inline(always)]
    pub fn sbt(&self) -> &Buffer {
        &self.sbt
    }

    #[inline(always)]
    pub fn raygen_offset(&self) -> u64 {
        self.table_bases.raygen
    }

    #[inline(always)]
    pub fn miss_offset(&self) -> u64 {
        self.table_bases.miss
    }

    #[inline(always)]
    pub fn hit_range(&self) -> Range<u64> {
        self.table_bases.hit_start..self.table_bases.hit_end
    }

    #[inline(always)]
    pub fn pipeline(&self) -> &RayTracingPipeline {
        &self.pipeline
    }

    pub fn check_for_rebuild<const FIF: usize>(
        &mut self,
        ctx: &Context,
        offset: &BindingTableOffset,
        materials: &ResourceAllocator<MaterialResource, FIF>,
        shaders: &ResourceAllocator<ShaderResource, FIF>,
    ) {
        if offset.binding_table_size() == self.last_table_size {
            return;
        }

        self.pipeline = Self::create_pipeline(
            ctx,
            RayTracingMaterialPipelineCreateInfo {
                pass: self.pass,
                layouts: self.layouts.clone(),
                materials,
                shaders,
                offset,
                raygen: self.raygen.clone(),
                miss: self.miss.clone(),
                recursion_depth: self.recursion_depth,
                push_constants: self.push_constants.clone(),
                debug_name: self.debug_name.clone(),
            },
        );

        let (sbt, table_bases) =
            Self::create_sbt(ctx, &self.pipeline, offset.binding_table_size() as usize);
        self.sbt = sbt;
        self.table_bases = table_bases;

        self.last_table_size = offset.binding_table_size();
    }

    fn create_pipeline<const FIF: usize>(
        ctx: &Context,
        create_info: RayTracingMaterialPipelineCreateInfo<FIF>,
    ) -> RayTracingPipeline {
        // Materials are allocated binding slots in order, and are never dropped.
        let mut stages = Vec::default();
        let mut groups = Vec::default();

        let mut binding_offset = 0;
        for material in create_info.materials.all().iter() {
            let material = match material {
                Some(mat) => mat,
                None => break,
            };

            // Grab the variants for this material
            let rt_variants = material.rt_variants.lock().unwrap();
            let variants = rt_variants.variants_of(create_info.pass);

            // Read until we run out of offsets.
            loop {
                let variant = match variants.get(&binding_offset) {
                    Some(v) => {
                        binding_offset += 1;
                        v
                    }
                    None => break,
                };

                // Add the stage and group
                groups.push(match variant.stage {
                    ShaderStage::RayClosestHit => RayTracingShaderGroup::Triangles {
                        closest_hit: Some(stages.len()),
                        any_hit: None,
                    },
                    ShaderStage::RayAnyHit => RayTracingShaderGroup::Triangles {
                        closest_hit: None,
                        any_hit: Some(stages.len()),
                    },
                    _ => panic!("invalid shader stage"),
                });

                stages.push(RayTracingShaderStage {
                    shader: create_info
                        .shaders
                        .get(variant.shader.id())
                        .unwrap()
                        .shader
                        .clone(),
                    stage: variant.stage,
                });
            }
        }

        // Must have gotten all the slots
        assert_eq!(
            binding_offset as usize,
            create_info.offset.binding_table_size()
        );

        // Add raygen and miss stage and group
        groups.push(RayTracingShaderGroup::RayGeneration(stages.len()));
        stages.push(RayTracingShaderStage {
            shader: create_info.raygen.clone(),
            stage: ShaderStage::RayGeneration,
        });

        groups.push(RayTracingShaderGroup::Miss(stages.len()));
        stages.push(RayTracingShaderStage {
            shader: create_info.miss.clone(),
            stage: ShaderStage::RayMiss,
        });

        // Create the pipeline
        let create_info = RayTracingPipelineCreateInfo {
            stages,
            groups,
            max_ray_recursion_depth: create_info.recursion_depth,
            layouts: create_info.layouts,
            push_constants_size: create_info.push_constants,
            debug_name: create_info.debug_name,
        };

        let pipeline = RayTracingPipeline::new(ctx.clone(), create_info).unwrap();

        pipeline
    }

    fn create_sbt(
        ctx: &Context,
        pipeline: &RayTracingPipeline,
        hit_group_count: usize,
    ) -> (Buffer, TableBases) {
        let sbt_data = pipeline.shader_binding_table_data();

        let mut table_bases = TableBases {
            hit_start: 0,
            hit_end: hit_group_count as u64 * sbt_data.aligned_size,
            raygen: (hit_group_count as u64 * sbt_data.aligned_size)
                .next_multiple_of(sbt_data.base_alignment),
            miss: 0,
        };
        table_bases.miss = table_bases.raygen
            + (sbt_data.aligned_size as u64).next_multiple_of(sbt_data.base_alignment);

        let mut buffer = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: table_bases.miss + sbt_data.aligned_size,
                array_elements: 1,
                buffer_usage: BufferUsage::SHADER_BINDING_TABLE | BufferUsage::DEVICE_ADDRESS,
                memory_usage: MemoryUsage::CpuToGpu,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("sbt".into()),
            },
        )
        .unwrap();

        // Copy in SBT hit group entries, respecting alignment
        let mut view = buffer.write(0).unwrap();
        let src = sbt_data.raw.chunks(sbt_data.entry_size as usize);
        let dst = view.chunks_mut(sbt_data.aligned_size as usize);

        for (dst, src) in dst.zip(src).take(hit_group_count) {
            dst[..sbt_data.entry_size as usize].copy_from_slice(src);
        }

        // Copy in raygen
        let dst_begin = table_bases.raygen as usize;
        let dst_end = (table_bases.raygen + sbt_data.entry_size) as usize;

        let src_begin = hit_group_count * sbt_data.entry_size as usize;
        let src_end = src_begin + sbt_data.entry_size as usize;
        view[dst_begin..dst_end].copy_from_slice(&sbt_data.raw[src_begin..src_end]);

        // Copy in miss
        let dst_begin = table_bases.miss as usize;
        let dst_end = (table_bases.miss + sbt_data.entry_size) as usize;

        let src_begin = (hit_group_count + 1) * sbt_data.entry_size as usize;
        let src_end = src_begin + sbt_data.entry_size as usize;
        view[dst_begin..dst_end].copy_from_slice(&sbt_data.raw[src_begin..src_end]);

        std::mem::drop(view);

        (buffer, table_bases)
    }
}
