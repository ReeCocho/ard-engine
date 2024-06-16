use std::ops::Range;

use ard_pal::prelude::*;
use ard_render_base::resource::ResourceAllocator;
use ard_render_material::{
    factory::{MaterialFactory, PassId},
    material::MaterialResource,
};

pub struct RayTracingMaterialPipelineCreateInfo<'a> {
    pub pass: PassId,
    pub layouts: Vec<DescriptorSetLayout>,
    pub materials: &'a ResourceAllocator<MaterialResource>,
    pub factory: &'a MaterialFactory,
    pub raygen: Shader,
    pub miss: Shader,
    pub debug_name: Option<String>,
}

pub struct RayTracingMaterialPipeline {
    sbt: Buffer,
    pass: PassId,
    pipeline: RayTracingPipeline,
    layouts: Vec<DescriptorSetLayout>,
    debug_name: Option<String>,
    raygen: Shader,
    miss: Shader,
    last_material_count: usize,
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
    pub fn new(ctx: &Context, create_info: RayTracingMaterialPipelineCreateInfo) -> Self {
        let pass = create_info.pass;
        let layouts = create_info.layouts.clone();
        let debug_name = create_info.debug_name.clone();
        let raygen = create_info.raygen.clone();
        let miss = create_info.miss.clone();
        let last_material_count = create_info.materials.allocated();

        let pipeline = Self::create_pipeline(ctx, create_info);
        let (sbt, table_bases) = Self::create_sbt(ctx, &pipeline, last_material_count);

        Self {
            sbt,
            pass,
            pipeline,
            layouts,
            debug_name,
            raygen,
            miss,
            last_material_count,
            table_bases,
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

    pub fn check_for_rebuild(
        &mut self,
        ctx: &Context,
        materials: &ResourceAllocator<MaterialResource>,
        factory: &MaterialFactory,
    ) {
        if materials.allocated() == self.last_material_count {
            return;
        }

        self.pipeline = Self::create_pipeline(
            ctx,
            RayTracingMaterialPipelineCreateInfo {
                pass: self.pass,
                layouts: self.layouts.clone(),
                materials,
                factory,
                raygen: self.raygen.clone(),
                miss: self.miss.clone(),
                debug_name: self.debug_name.clone(),
            },
        );

        let (sbt, table_bases) = Self::create_sbt(ctx, &self.pipeline, materials.allocated());
        self.sbt = sbt;
        self.table_bases = table_bases;

        self.last_material_count = materials.allocated();
    }

    fn create_pipeline(
        ctx: &Context,
        create_info: RayTracingMaterialPipelineCreateInfo,
    ) -> RayTracingPipeline {
        // Stage and group for raygen and miss shader
        let stages = vec![
            RayTracingShaderStage {
                shader: create_info.raygen.clone(),
                stage: ShaderStage::RayGeneration,
            },
            RayTracingShaderStage {
                shader: create_info.miss.clone(),
                stage: ShaderStage::RayMiss,
            },
        ];

        let groups = vec![
            RayTracingShaderGroup::RayGeneration(0),
            RayTracingShaderGroup::Miss(1),
        ];

        // Materials are all pipeline libraries
        let mut libraries = Vec::default();
        for material in create_info.materials.all().iter() {
            let material = match material.resource.as_ref() {
                Some(mat) => mat,
                None => break,
            };

            libraries.push(
                material
                    .rt_variants
                    .get(&create_info.pass)
                    .unwrap()
                    .pipeline
                    .clone(),
            );
        }

        // Must have gotten all the slots
        assert_eq!(libraries.len(), create_info.materials.allocated());

        // Create the pipeline
        let pass = create_info.factory.get_rt_pass(create_info.pass).unwrap();
        let create_info = RayTracingPipelineCreateInfo {
            stages,
            groups,
            max_ray_recursion_depth: pass.max_ray_recursion,
            layouts: create_info.layouts,
            push_constants_size: pass.push_constant_size,
            library_info: Some(PipelineLibraryInfo {
                is_library: false,
                max_ray_payload_size: pass.max_ray_payload_size,
                max_ray_hit_attribute_size: pass.max_ray_hit_attribute_size,
            }),
            libraries,
            debug_name: create_info.debug_name,
        };

        let pipeline = RayTracingPipeline::new(ctx.clone(), create_info).unwrap();

        pipeline
    }

    fn create_sbt(
        ctx: &Context,
        pipeline: &RayTracingPipeline,
        material_count: usize,
    ) -> (Buffer, TableBases) {
        let sbt_data = pipeline.shader_binding_table_data();

        let mut table_bases = TableBases {
            // Raygen shader comes first.
            raygen: 0,
            // Followed by the miss shader.
            miss: sbt_data
                .aligned_size
                .next_multiple_of(sbt_data.base_alignment),
            hit_start: 0,
            hit_end: 0,
        };
        // Followed by every materials variants.
        table_bases.hit_start = table_bases.miss
            + sbt_data
                .aligned_size
                .next_multiple_of(sbt_data.base_alignment);
        table_bases.hit_end = table_bases.hit_start
            + (material_count as u64
                * MaterialResource::RT_GROUPS_PER_MATERIAL as u64
                * sbt_data.aligned_size);

        let mut buffer = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: table_bases.hit_end,
                array_elements: 1,
                buffer_usage: BufferUsage::SHADER_BINDING_TABLE | BufferUsage::DEVICE_ADDRESS,
                memory_usage: MemoryUsage::CpuToGpu,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("sbt".into()),
            },
        )
        .unwrap();

        let mut view = buffer.write(0).unwrap();

        // Copy in raygen
        let dst_begin = table_bases.raygen as usize;
        let dst_end = (table_bases.raygen + sbt_data.entry_size) as usize;

        let src_begin = 0;
        let src_end = sbt_data.entry_size as usize;
        view[dst_begin..dst_end].copy_from_slice(&sbt_data.raw[src_begin..src_end]);

        // Copy in miss
        let dst_begin = table_bases.miss as usize;
        let dst_end = (table_bases.miss + sbt_data.entry_size) as usize;

        let src_begin = sbt_data.entry_size as usize;
        let src_end = 2 * sbt_data.entry_size as usize;
        view[dst_begin..dst_end].copy_from_slice(&sbt_data.raw[src_begin..src_end]);

        // Copy in SBT hit group entries, respecting alignment
        for i in 0..(material_count * MaterialResource::RT_GROUPS_PER_MATERIAL) {
            let dst_begin = table_bases.hit_start as usize + (i * sbt_data.aligned_size as usize);
            let dst_end = dst_begin + sbt_data.entry_size as usize;

            let src_begin = (2 + i) * sbt_data.entry_size as usize;
            let src_end = src_begin + sbt_data.entry_size as usize;
            view[dst_begin..dst_end].copy_from_slice(&sbt_data.raw[src_begin..src_end]);
        }

        std::mem::drop(view);

        (buffer, table_bases)
    }
}
