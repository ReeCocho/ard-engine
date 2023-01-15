use std::ops::DerefMut;

use ard_core::prelude::Disabled;
use ard_ecs::prelude::*;
use ard_formats::mesh::VertexLayout;
use ard_math::{Mat4, Vec2, Vec4, Vec4Swizzles};
use ard_pal::prelude::*;
use bytemuck::{Pod, Zeroable};
use ordered_float::NotNan;

use crate::{
    camera::{CameraIbl, CameraUbo},
    cube_map::CubeMapInner,
    factory::{
        allocator::{ResourceAllocator, ResourceId},
        materials::MaterialBuffers,
        meshes::MeshBuffers,
        textures::TextureSets,
        Factory, Layouts,
    },
    lighting::{Lighting, PointLight, RawLight, RawPointLight},
    material::{Material, MaterialInner, MaterialInstance, PipelineType},
    mesh::{Mesh, MeshInner, ObjectBounds},
    shader_constants::{FRAMES_IN_FLIGHT, FROXEL_TABLE_DIMS, MAX_SHADOW_CASCADES},
    static_geometry::StaticGeometryInner,
};

use super::{
    ao::AO_SAMPLER,
    clustering::{
        LightClustering, LightClusteringPushConstants, FROXEL_GEN_CAMERA_BINDING,
        FROXEL_GEN_CLUSTERS_BINDING, LIGHT_CLUSTERING_CAMERA_BINDING,
        LIGHT_CLUSTERING_CLUSTERS_BINDING, LIGHT_CLUSTERING_FROXELS_BINDING,
        LIGHT_CLUSTERING_LIGHTS_BINDING,
    },
    occlusion::HzbImage,
    shadows::{Shadows, SHADOW_SAMPLER},
    Model, RenderLayer, Renderable,
};

const DEFAULT_OBJECT_DATA_CAP: u64 = 3;
const DEFAULT_INPUT_ID_CAP: u64 = 3;
const DEFAULT_OUTPUT_ID_CAP: u64 = 3;
const DEFAULT_DRAW_CALL_CAP: u64 = 3;
const DEFAULT_LIGHT_CAP: u64 = 3;

const GLOBAL_OBJECT_DATA_BINDING: u32 = 0;
const GLOBAL_OBJECT_ID_BINDING: u32 = 1;
const GLOBAL_LIGHTS_BINDING: u32 = 2;
const GLOBAL_CLUSTER_BINDING: u32 = 3;
const GLOBAL_LIGHTING_BINDING: u32 = 4;
const GLOBAL_BRDF_LUT_BINDING: u32 = 5;
const GLOBAL_DIFFUSE_IRRADIANCE_MAP_BINDING: u32 = 6;
const GLOBAL_PREFILTERED_ENV_MAP_BINDING: u32 = 7;

const DRAW_GEN_DRAW_CALLS_BINDING: u32 = 0;
const DRAW_GEN_OBJECT_DATA_BINDING: u32 = 1;
const DRAW_GEN_INPUT_ID_BINDING: u32 = 2;
const DRAW_GEN_OUTPUT_ID_BINDING: u32 = 3;
const DRAW_GEN_CAMERA_BINDING: u32 = 4;
const DRAW_GEN_HZB_BINDING: u32 = 5;

const CAMERA_UBO_BINDING: u32 = 0;
const CAMERA_SHADOW_INFO_BINDING: u32 = 1;
const CAMERA_SHADOW_MAPS_BINDING: u32 = 2;
const CAMERA_AO_BINDING: u32 = 3;

const SKYBOX_CAMERA_BINDING: u32 = 0;
const SKYBOX_CUBE_MAP_BINDING: u32 = 1;

const DRAW_GEN_WORKGROUP_SIZE: u32 = 256;

pub(crate) const SKY_BOX_SAMPLER: Sampler = Sampler {
    min_filter: Filter::Linear,
    mag_filter: Filter::Linear,
    mipmap_filter: Filter::Linear,
    address_u: SamplerAddressMode::ClampToEdge,
    address_v: SamplerAddressMode::ClampToEdge,
    address_w: SamplerAddressMode::ClampToEdge,
    anisotropy: None,
    compare: None,
    min_lod: unsafe { NotNan::new_unchecked(0.0) },
    max_lod: None,
    border_color: None,
    unnormalize_coords: false,
};

pub(crate) const BRDF_LUT_SAMPLER: Sampler = Sampler {
    min_filter: Filter::Linear,
    mag_filter: Filter::Linear,
    mipmap_filter: Filter::Linear,
    address_u: SamplerAddressMode::ClampToEdge,
    address_v: SamplerAddressMode::ClampToEdge,
    address_w: SamplerAddressMode::ClampToEdge,
    anisotropy: None,
    compare: None,
    min_lod: unsafe { NotNan::new_unchecked(0.0) },
    max_lod: None,
    border_color: None,
    unnormalize_coords: false,
};

pub(crate) struct GlobalRenderData {
    /// Layout for global rendering data.
    pub global_layout: DescriptorSetLayout,
    /// Layout for draw call generation.
    pub draw_gen_layout: DescriptorSetLayout,
    /// Layout for light clustering.
    pub light_cluster_layout: DescriptorSetLayout,
    /// Layout for camera froxel generation.
    pub froxel_gen_layout: DescriptorSetLayout,
    /// Layout for camera view information.
    pub camera_layout: DescriptorSetLayout,
    /// Layout for skybox rendering.
    pub sky_box_layout: DescriptorSetLayout,
    /// Pipeline to perform draw call generation.
    pub draw_gen_pipeline: ComputePipeline,
    /// Pipeline to perform draw call generation without high-z culling.
    pub draw_gen_no_hzb_pipeline: ComputePipeline,
    /// Pipeline to cluster lights.
    pub light_cluster_pipeline: ComputePipeline,
    /// Pipeline to generate camera froxels.
    pub froxel_gen_pipeline: ComputePipeline,
    /// Pipeline to render the skybox.
    pub sky_box_pipeline: GraphicsPipeline,
    /// BRDF look-up texture.
    pub brdf_lut: Texture,
    /// White cube map.
    pub white_cube_map: CubeMap,
    /// Empty shadow map for unbound cascades.
    pub empty_shadow: Texture,
    /// Contains all object data.
    pub object_data: Buffer,
    /// Contains all lights.
    pub lights: Buffer,
    /// The total number of lights.
    pub light_count: usize,
}

pub(crate) struct RenderData {
    /// Global rendering data descriptor sets.
    pub global_sets: Vec<DescriptorSet>,
    /// Sets for the camera UBO.
    pub camera_sets: Vec<DescriptorSet>,
    /// Draw generation descriptor sets.
    pub draw_gen_sets: Vec<DescriptorSet>,
    /// Sets for skybox rendering.
    pub sky_box_sets: Vec<DescriptorSet>,
    /// Draw keys used during render sorting. Holds the key and number of objects that use the key.
    pub keys: [Vec<(DrawKey, usize)>; FRAMES_IN_FLIGHT],
    /// Optional data for light clustering.
    pub clustering: Option<LightClustering>,
    /// UBO for camera data.
    pub camera_ubo: Buffer,
    /// Contains input IDs that the GPU will parse in draw generation.
    pub input_ids: Buffer,
    /// Generated by the GPU. Contains the indices into the primary object info array for all
    /// objects to render.
    pub output_ids: Buffer,
    /// Generated by the GPU. Contains indirect draw calls to perform.
    pub draw_calls: Buffer,
    /// Scratch space to hold static input ids.
    static_input_ids: Vec<InputObjectId>,
    /// Scratch space to hold dynamic input ids for sorting.
    dynamic_input_ids: Vec<InputObjectId>,
    /// The number of static objects detected for rendering.
    pub static_objects: usize,
    /// The number of dynamic objects detected for rendering.
    pub dynamic_objects: usize,
    /// The number of static draw calls from last frame.
    pub last_static_draws: usize,
    /// The total number of static draw calls.
    pub static_draws: usize,
    /// The total number of dynamic draw calls.
    pub dynamic_draws: usize,
}

pub(crate) struct RenderArgs<'a, 'b> {
    pub pass: &'b mut RenderPass<'a>,
    pub texture_sets: &'a TextureSets,
    pub material_buffers: &'a MaterialBuffers,
    pub mesh_buffers: &'a MeshBuffers,
    pub materials: &'a ResourceAllocator<MaterialInner>,
    pub meshes: &'a ResourceAllocator<MeshInner>,
    pub global: &'a GlobalRenderData,
    pub draw_sky_box: bool,
    pub pipeline_ty: PipelineType,
    pub draw_offset: usize,
    pub draw_count: usize,
    pub material_override: Option<Material>,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct DrawGenPushConstants {
    render_area: Vec2,
    object_count: u32,
}

/// Information to draw an object.
#[repr(C, align(16))]
#[derive(Copy, Clone)]
struct ObjectData {
    /// Model matrix of the object.
    pub model: Mat4,
    /// Normal matrix of the object.
    pub normal: Mat4,
    /// Index into the material buffer for this objects material. `NO_MATERIAL` if none.
    pub material: u32,
    /// Index into the textures buffer for this objects material. `NO_TEXTURES` if none.
    pub textures: u32,
    /// ID of the entity of this object.
    pub entity_id: u32,
    /// Version of the entity of this object.
    pub entity_ver: u32,
}

/// Draw calls are unique to each material/mesh combo.
#[repr(C)]
#[derive(Copy, Clone)]
struct DrawCall {
    /// Draw call for this object type.
    pub indirect: DrawIndexedIndirect,
    /// Object bounds for the mesh.
    pub bounds: ObjectBounds,
}

/// An object to be processed during draw call generations.
#[repr(C, align(16))]
#[derive(Copy, Clone)]
struct InputObjectId {
    /// Index into the `draw_calls` buffer of `RenderData` for what draw call this object belongs
    /// to.
    ///
    /// ## Note
    /// You might be wondering why this is an array. Well, in order to generate dynamic draw calls
    /// we need to sort all the objects by their draw key and then compact duplicates into single
    /// draws. In order to do this, all the objects must know what their batch index is "before we
    /// actually generate them" (this is mostly for performance reasons). With static objects it
    /// isn't an issue because they are already sorted. For dynamic objects we must sort them
    /// ourselves. To do this, we use this field to hold the draw key. Since the draw key is a
    /// 64-bit number, we need two u32 fields to hold it.
    pub draw_idx: [u32; 2],
    /// Index into the `object_data` buffer of `GlobalRenderData` for this object.
    pub data_idx: u32,
    pub _dummy: u32,
}

/// Index into the `object_data` buffer in `GlobalRenderData`.
pub type OutputObjectId = u32;

/// Used to sort draw calls.
pub type DrawKey = u64;

unsafe impl Zeroable for ObjectData {}
unsafe impl Pod for ObjectData {}

unsafe impl Zeroable for DrawCall {}
unsafe impl Pod for DrawCall {}

unsafe impl Zeroable for InputObjectId {}
unsafe impl Pod for InputObjectId {}

unsafe impl Zeroable for DrawGenPushConstants {}
unsafe impl Pod for DrawGenPushConstants {}

impl GlobalRenderData {
    pub fn new(ctx: &Context) -> Self {
        let object_data = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of::<ObjectData>() as u64 * DEFAULT_OBJECT_DATA_CAP,
                array_elements: FRAMES_IN_FLIGHT,
                buffer_usage: BufferUsage::STORAGE_BUFFER,
                memory_usage: MemoryUsage::CpuToGpu,
                debug_name: Some(String::from("object_data")),
            },
        )
        .unwrap();

        let lights = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of::<RawLight>() as u64 * DEFAULT_LIGHT_CAP,
                array_elements: FRAMES_IN_FLIGHT,
                buffer_usage: BufferUsage::STORAGE_BUFFER,
                memory_usage: MemoryUsage::CpuToGpu,
                debug_name: Some(String::from("lights")),
            },
        )
        .unwrap();

        let camera_layout = DescriptorSetLayout::new(
            ctx.clone(),
            DescriptorSetLayoutCreateInfo {
                bindings: vec![
                    // UBO
                    DescriptorBinding {
                        binding: CAMERA_UBO_BINDING,
                        ty: DescriptorType::UniformBuffer,
                        count: 1,
                        stage: ShaderStage::AllGraphics,
                    },
                    // Shadow info
                    DescriptorBinding {
                        binding: CAMERA_SHADOW_INFO_BINDING,
                        ty: DescriptorType::UniformBuffer,
                        count: 1,
                        stage: ShaderStage::AllGraphics,
                    },
                    // Shadow maps
                    DescriptorBinding {
                        binding: CAMERA_SHADOW_MAPS_BINDING,
                        ty: DescriptorType::Texture,
                        count: MAX_SHADOW_CASCADES,
                        stage: ShaderStage::AllGraphics,
                    },
                    // AO texture
                    DescriptorBinding {
                        binding: CAMERA_AO_BINDING,
                        ty: DescriptorType::Texture,
                        count: 1,
                        stage: ShaderStage::AllGraphics,
                    },
                ],
            },
        )
        .unwrap();

        let light_cluster_layout = DescriptorSetLayout::new(
            ctx.clone(),
            DescriptorSetLayoutCreateInfo {
                bindings: vec![
                    // Lights
                    DescriptorBinding {
                        binding: LIGHT_CLUSTERING_LIGHTS_BINDING,
                        ty: DescriptorType::StorageBuffer(AccessType::Read),
                        count: 1,
                        stage: ShaderStage::Compute,
                    },
                    // Clusters
                    DescriptorBinding {
                        binding: LIGHT_CLUSTERING_CLUSTERS_BINDING,
                        ty: DescriptorType::StorageBuffer(AccessType::ReadWrite),
                        count: 1,
                        stage: ShaderStage::Compute,
                    },
                    // Camera
                    DescriptorBinding {
                        binding: LIGHT_CLUSTERING_CAMERA_BINDING,
                        ty: DescriptorType::UniformBuffer,
                        count: 1,
                        stage: ShaderStage::Compute,
                    },
                    // Froxels
                    DescriptorBinding {
                        binding: LIGHT_CLUSTERING_FROXELS_BINDING,
                        ty: DescriptorType::StorageBuffer(AccessType::Read),
                        count: 1,
                        stage: ShaderStage::Compute,
                    },
                ],
            },
        )
        .unwrap();

        let froxel_gen_layout = DescriptorSetLayout::new(
            ctx.clone(),
            DescriptorSetLayoutCreateInfo {
                bindings: vec![
                    // UBO
                    DescriptorBinding {
                        binding: FROXEL_GEN_CAMERA_BINDING,
                        ty: DescriptorType::UniformBuffer,
                        count: 1,
                        stage: ShaderStage::Compute,
                    },
                    // Clusters
                    DescriptorBinding {
                        binding: FROXEL_GEN_CLUSTERS_BINDING,
                        ty: DescriptorType::StorageBuffer(AccessType::ReadWrite),
                        count: 1,
                        stage: ShaderStage::Compute,
                    },
                ],
            },
        )
        .unwrap();

        let global_layout = DescriptorSetLayout::new(
            ctx.clone(),
            DescriptorSetLayoutCreateInfo {
                bindings: vec![
                    // Object data
                    DescriptorBinding {
                        binding: GLOBAL_OBJECT_DATA_BINDING,
                        ty: DescriptorType::StorageBuffer(AccessType::Read),
                        count: 1,
                        stage: ShaderStage::AllGraphics,
                    },
                    // Object IDs
                    DescriptorBinding {
                        binding: GLOBAL_OBJECT_ID_BINDING,
                        ty: DescriptorType::StorageBuffer(AccessType::Read),
                        count: 1,
                        stage: ShaderStage::AllGraphics,
                    },
                    // Lights
                    DescriptorBinding {
                        binding: GLOBAL_LIGHTS_BINDING,
                        ty: DescriptorType::StorageBuffer(AccessType::Read),
                        count: 1,
                        stage: ShaderStage::AllGraphics,
                    },
                    // Clusters
                    DescriptorBinding {
                        binding: GLOBAL_CLUSTER_BINDING,
                        ty: DescriptorType::StorageBuffer(AccessType::Read),
                        count: 1,
                        stage: ShaderStage::AllGraphics,
                    },
                    // Lighting
                    DescriptorBinding {
                        binding: GLOBAL_LIGHTING_BINDING,
                        ty: DescriptorType::UniformBuffer,
                        count: 1,
                        stage: ShaderStage::AllGraphics,
                    },
                    // Diffuse irradiance map
                    DescriptorBinding {
                        binding: GLOBAL_BRDF_LUT_BINDING,
                        ty: DescriptorType::Texture,
                        count: 1,
                        stage: ShaderStage::AllGraphics,
                    },
                    // Diffuse irradiance map
                    DescriptorBinding {
                        binding: GLOBAL_DIFFUSE_IRRADIANCE_MAP_BINDING,
                        ty: DescriptorType::CubeMap,
                        count: 1,
                        stage: ShaderStage::AllGraphics,
                    },
                    // Prefiltered environment map
                    DescriptorBinding {
                        binding: GLOBAL_PREFILTERED_ENV_MAP_BINDING,
                        ty: DescriptorType::CubeMap,
                        count: 1,
                        stage: ShaderStage::AllGraphics,
                    },
                ],
            },
        )
        .unwrap();

        let draw_gen_layout = DescriptorSetLayout::new(
            ctx.clone(),
            DescriptorSetLayoutCreateInfo {
                bindings: vec![
                    // Draw calls
                    DescriptorBinding {
                        binding: DRAW_GEN_DRAW_CALLS_BINDING,
                        ty: DescriptorType::StorageBuffer(AccessType::ReadWrite),
                        count: 1,
                        stage: ShaderStage::Compute,
                    },
                    // Object data
                    DescriptorBinding {
                        binding: DRAW_GEN_OBJECT_DATA_BINDING,
                        ty: DescriptorType::StorageBuffer(AccessType::Read),
                        count: 1,
                        stage: ShaderStage::Compute,
                    },
                    // Input IDs
                    DescriptorBinding {
                        binding: DRAW_GEN_INPUT_ID_BINDING,
                        ty: DescriptorType::StorageBuffer(AccessType::Read),
                        count: 1,
                        stage: ShaderStage::Compute,
                    },
                    // Output IDs
                    DescriptorBinding {
                        binding: DRAW_GEN_OUTPUT_ID_BINDING,
                        ty: DescriptorType::StorageBuffer(AccessType::ReadWrite),
                        count: 1,
                        stage: ShaderStage::Compute,
                    },
                    // Camera
                    DescriptorBinding {
                        binding: DRAW_GEN_CAMERA_BINDING,
                        ty: DescriptorType::UniformBuffer,
                        count: 1,
                        stage: ShaderStage::Compute,
                    },
                    // HZB image
                    DescriptorBinding {
                        binding: DRAW_GEN_HZB_BINDING,
                        ty: DescriptorType::Texture,
                        count: 1,
                        stage: ShaderStage::Compute,
                    },
                ],
            },
        )
        .unwrap();

        let sky_box_layout = DescriptorSetLayout::new(
            ctx.clone(),
            DescriptorSetLayoutCreateInfo {
                bindings: vec![
                    // Camera
                    DescriptorBinding {
                        binding: SKYBOX_CAMERA_BINDING,
                        ty: DescriptorType::UniformBuffer,
                        count: 1,
                        stage: ShaderStage::Vertex,
                    },
                    // Sky box
                    DescriptorBinding {
                        binding: SKYBOX_CUBE_MAP_BINDING,
                        ty: DescriptorType::CubeMap,
                        count: 1,
                        stage: ShaderStage::Fragment,
                    },
                ],
            },
        )
        .unwrap();

        let draw_gen_shader = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!("../shaders/draw_gen.comp.spv"),
                debug_name: Some(String::from("draw_gen_shader")),
            },
        )
        .unwrap();

        let draw_gen_pipeline = ComputePipeline::new(
            ctx.clone(),
            ComputePipelineCreateInfo {
                layouts: vec![draw_gen_layout.clone()],
                module: draw_gen_shader,
                work_group_size: (DRAW_GEN_WORKGROUP_SIZE, 1, 1),
                push_constants_size: Some(std::mem::size_of::<DrawGenPushConstants>() as u32),
                debug_name: Some(String::from("draw_gen_pipeline")),
            },
        )
        .unwrap();

        let draw_gen_shader = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!("../shaders/draw_gen_no_highz.comp.spv"),
                debug_name: Some(String::from("draw_gen_no_hzb_shader")),
            },
        )
        .unwrap();

        let draw_gen_no_hzb_pipeline = ComputePipeline::new(
            ctx.clone(),
            ComputePipelineCreateInfo {
                layouts: vec![draw_gen_layout.clone()],
                module: draw_gen_shader,
                work_group_size: (DRAW_GEN_WORKGROUP_SIZE, 1, 1),
                push_constants_size: Some(std::mem::size_of::<DrawGenPushConstants>() as u32),
                debug_name: Some(String::from("draw_gen_no_hzb_pipeline")),
            },
        )
        .unwrap();

        let froxel_gen_shader = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!("../shaders/froxel_gen.comp.spv"),
                debug_name: Some(String::from("froxel_gen_shader")),
            },
        )
        .unwrap();

        let froxel_gen_pipeline = ComputePipeline::new(
            ctx.clone(),
            ComputePipelineCreateInfo {
                layouts: vec![froxel_gen_layout.clone()],
                module: froxel_gen_shader,
                work_group_size: (1, 1, 1),
                push_constants_size: None,
                debug_name: Some(String::from("froxel_gen_pipeline")),
            },
        )
        .unwrap();

        let light_cluster_shader = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!("../shaders/light_clustering.comp.spv"),
                debug_name: Some(String::from("light_clustering_shader")),
            },
        )
        .unwrap();

        let light_cluster_pipeline = ComputePipeline::new(
            ctx.clone(),
            ComputePipelineCreateInfo {
                layouts: vec![light_cluster_layout.clone()],
                module: light_cluster_shader,
                work_group_size: (FROXEL_TABLE_DIMS.0 as u32, FROXEL_TABLE_DIMS.1 as u32, 1),
                push_constants_size: Some(
                    std::mem::size_of::<LightClusteringPushConstants>() as u32
                ),
                debug_name: Some(String::from("light_clustering_pipeline")),
            },
        )
        .unwrap();

        let skybox_frag_shader = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!("../shaders/skybox.frag.spv"),
                debug_name: Some(String::from("sky_box_fragment_shader")),
            },
        )
        .unwrap();

        let skybox_vert_shader = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!("../shaders/skybox.vert.spv"),
                debug_name: Some(String::from("sky_box_vertex_shader")),
            },
        )
        .unwrap();

        let sky_box_pipeline = GraphicsPipeline::new(
            ctx.clone(),
            GraphicsPipelineCreateInfo {
                stages: ShaderStages {
                    vertex: skybox_vert_shader,
                    fragment: Some(skybox_frag_shader),
                },
                layouts: vec![sky_box_layout.clone()],
                vertex_input: VertexInputState {
                    attributes: Vec::default(),
                    bindings: Vec::default(),
                    topology: PrimitiveTopology::TriangleList,
                },
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::None,
                    front_face: FrontFace::Clockwise,
                },
                depth_stencil: Some(DepthStencilState {
                    depth_clamp: false,
                    depth_test: false,
                    depth_write: false,
                    depth_compare: CompareOp::Always,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }),
                color_blend: Some(ColorBlendState {
                    attachments: vec![ColorBlendAttachment {
                        blend: false,
                        write_mask: ColorComponents::R
                            | ColorComponents::G
                            | ColorComponents::B
                            | ColorComponents::A,
                        ..Default::default()
                    }],
                }),
                push_constants_size: None,
                debug_name: Some(String::from("sky_box_pipeline")),
            },
        )
        .unwrap();

        // Create default images
        let brdf_lut = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: TextureFormat::Rg16SFloat,
                ty: TextureType::Type2D,
                width: 512,
                height: 512,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                texture_usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("brdf_lut")),
            },
        )
        .unwrap();

        let empty_shadow = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: TextureFormat::D32Sfloat,
                ty: TextureType::Type2D,
                width: 1,
                height: 1,
                depth: 1,
                array_elements: FRAMES_IN_FLIGHT,
                mip_levels: 1,
                texture_usage: TextureUsage::DEPTH_STENCIL_ATTACHMENT | TextureUsage::SAMPLED,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("empty_shadow")),
            },
        )
        .unwrap();

        let white_cube_map = CubeMap::new(
            ctx.clone(),
            CubeMapCreateInfo {
                format: TextureFormat::Rgba8Unorm,
                size: 1,
                array_elements: 1,
                mip_levels: 1,
                texture_usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("white_cube_map")),
            },
        )
        .unwrap();

        let white_cube_map_staging = Buffer::new_staging(
            ctx.clone(),
            Some(String::from("white_cube_map_staging")),
            // 1x1 face, 6 faces, each pixel is 4 bytes so 6 * 4 elements
            &[255; 4 * 6],
        )
        .unwrap();

        let brdf_lut_staging = Buffer::new_staging(
            ctx.clone(),
            Some(String::from("brdf_lut_staging")),
            include_bytes!("./brdf_lut.bin"),
        )
        .unwrap();

        let mut command_buffer = ctx.main().command_buffer();
        for frame in 0..FRAMES_IN_FLIGHT {
            command_buffer.render_pass(
                RenderPassDescriptor {
                    color_attachments: Vec::default(),
                    depth_stencil_attachment: Some(DepthStencilAttachment {
                        texture: &empty_shadow,
                        array_element: frame,
                        mip_level: 0,
                        load_op: LoadOp::Clear(ClearColor::D32S32(1.0, 0)),
                        store_op: StoreOp::Store,
                    }),
                },
                |_pass| {},
            );
        }

        command_buffer.copy_buffer_to_cube_map(
            &white_cube_map,
            &white_cube_map_staging,
            BufferCubeMapCopy {
                buffer_offset: 0,
                buffer_array_element: 0,
                cube_map_mip_level: 0,
                cube_map_array_element: 0,
            },
        );

        command_buffer.copy_buffer_to_texture(
            &brdf_lut,
            &brdf_lut_staging,
            BufferTextureCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                buffer_array_element: 0,
                texture_offset: (0, 0, 0),
                texture_extent: (512, 512, 1),
                texture_mip_level: 0,
                texture_array_element: 0,
            },
        );

        ctx.main()
            .submit(Some("empty_shadow_prepare"), command_buffer);

        Self {
            global_layout,
            draw_gen_layout,
            light_cluster_layout,
            camera_layout,
            froxel_gen_layout,
            sky_box_layout,
            draw_gen_pipeline,
            draw_gen_no_hzb_pipeline,
            light_cluster_pipeline,
            froxel_gen_pipeline,
            sky_box_pipeline,
            object_data,
            brdf_lut,
            empty_shadow,
            white_cube_map,
            lights,
            light_count: 0,
        }
    }

    /// Writes all possibly rendered objects into the global buffer. If the buffer was reszied,
    /// `true` is returned.
    pub fn prepare_object_data(
        &mut self,
        frame: usize,
        factory: &Factory,
        queries: &Queries<Everything>,
        static_geometry: &StaticGeometryInner,
    ) -> bool {
        let query = queries.make::<(Entity, (Read<Renderable>, Read<Model>))>();
        let materials = factory.0.material_instances.lock().unwrap();

        // Expand object data buffer if required
        let obj_count = query.len() + static_geometry.len;
        let expanded = match Buffer::expand(
            &self.object_data,
            (obj_count * std::mem::size_of::<ObjectData>()) as u64,
            false,
        ) {
            Some(buffer) => {
                self.object_data = buffer;
                true
            }
            None => true,
        };

        // Write in every object
        let mut view = self.object_data.write(frame).unwrap();
        let slice = bytemuck::cast_slice_mut::<_, ObjectData>(&mut view);

        // Write in static geometry if it's dirty
        if static_geometry.dirty[frame] {
            let mut cur_offset = 0;
            for key in &static_geometry.sorted_keys {
                let batch = static_geometry.batches.get(key).unwrap();
                let material = materials.get(batch.renderable.material.id).unwrap();
                for i in 0..batch.ids.len() {
                    let entity = batch.entities[i];
                    slice[cur_offset] = ObjectData {
                        model: batch.models[i],
                        normal: batch.models[i].inverse().transpose(),
                        material: material
                            .material_block
                            .map(|block| block.into())
                            .unwrap_or(0),
                        textures: material
                            .texture_block
                            .map(|block| block.into())
                            .unwrap_or(0),
                        entity_id: entity.id(),
                        entity_ver: entity.ver(),
                    };
                    cur_offset += 1;
                }
            }
        }

        // Write in dynamic geometry
        for (i, (entity, (renderable, model))) in query.into_iter().enumerate() {
            let material = materials.get(renderable.material.id).unwrap();
            slice[i + static_geometry.len] = ObjectData {
                model: model.0,
                normal: model.0.inverse().transpose(),
                material: material
                    .material_block
                    .map(|block| block.into())
                    .unwrap_or(0),
                textures: material
                    .texture_block
                    .map(|block| block.into())
                    .unwrap_or(0),
                entity_id: entity.id(),
                entity_ver: entity.ver(),
            };
        }

        expanded
    }

    /// Writes all possibly rendered lights into the global buffer.
    pub fn prepare_lights(&mut self, frame: usize, queries: &Queries<Everything>) -> bool {
        let query = queries.make::<(Entity, (Read<PointLight>, Read<Model>), (Read<Disabled>,))>();

        // Expand light buffer if needed
        let expanded = match Buffer::expand(
            &self.lights,
            (query.len() * std::mem::size_of::<RawLight>()) as u64,
            false,
        ) {
            Some(buffer) => {
                self.lights = buffer;
                true
            }
            None => true,
        };

        // Write in lights
        self.light_count = 0;
        let mut view = self.lights.write(frame).unwrap();
        let slice = bytemuck::cast_slice_mut::<_, RawLight>(&mut view);
        for (i, (_, (light, model), _)) in query
            .into_iter()
            .filter(|(_, _, (disabled,))| disabled.is_none())
            .enumerate()
        {
            slice[i] = RawLight {
                point: RawPointLight {
                    color_intensity: Vec4::from((light.color, light.intensity)),
                    position_range: Vec4::from((model.0.col(3).xyz(), light.range)),
                },
            };
            self.light_count += 1;
        }

        expanded
    }
}

impl RenderData {
    pub fn new(ctx: &Context, name: &str, layouts: &Layouts, use_clustered_lighting: bool) -> Self {
        // Create buffers
        let camera_ubo = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of::<CameraUbo>() as u64,
                array_elements: FRAMES_IN_FLIGHT,
                buffer_usage: BufferUsage::UNIFORM_BUFFER,
                memory_usage: MemoryUsage::CpuToGpu,
                debug_name: Some(String::from("camera_ubo")),
            },
        )
        .unwrap();

        let input_ids = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of::<InputObjectId>() as u64 * DEFAULT_INPUT_ID_CAP,
                array_elements: FRAMES_IN_FLIGHT,
                buffer_usage: BufferUsage::STORAGE_BUFFER,
                memory_usage: MemoryUsage::CpuToGpu,
                debug_name: Some(String::from("input_ids")),
            },
        )
        .unwrap();

        let output_ids = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of::<OutputObjectId>() as u64 * DEFAULT_OUTPUT_ID_CAP,
                array_elements: 1,
                buffer_usage: BufferUsage::STORAGE_BUFFER,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("output_ids")),
            },
        )
        .unwrap();

        let draw_calls = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of::<DrawCall>() as u64 * DEFAULT_DRAW_CALL_CAP,
                // We need two draw call buffers per frame in flight because we alternate between
                // them for use in occlusion culling.
                array_elements: FRAMES_IN_FLIGHT * 2,
                buffer_usage: BufferUsage::STORAGE_BUFFER | BufferUsage::INDIRECT_BUFFER,
                memory_usage: MemoryUsage::CpuToGpu,
                debug_name: Some(String::from("draw_calls")),
            },
        )
        .unwrap();

        // Create clustered lighting if required
        let clustering = if use_clustered_lighting {
            Some(LightClustering::new(
                ctx,
                name,
                &camera_ubo,
                &layouts.light_cluster,
                &layouts.froxel_gen,
            ))
        } else {
            None
        };

        // Create global sets
        let mut global_sets = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for frame in 0..FRAMES_IN_FLIGHT {
            let mut set = DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: layouts.global.clone(),
                    debug_name: Some(format!("{name}_global_set_a_{frame}")),
                },
            )
            .unwrap();

            if let Some(clustering) = &clustering {
                set.update(&[DescriptorSetUpdate {
                    binding: GLOBAL_CLUSTER_BINDING,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer: &clustering.light_table,
                        array_element: 0,
                    },
                }]);
            }

            global_sets.push(set);
        }

        // Create draw generation sets
        let mut draw_gen_sets = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for frame in 0..FRAMES_IN_FLIGHT {
            let mut set = DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: layouts.draw_gen.clone(),
                    debug_name: Some(format!("{name}_draw_gen_set_{frame}")),
                },
            )
            .unwrap();

            set.update(&[DescriptorSetUpdate {
                binding: DRAW_GEN_CAMERA_BINDING,
                array_element: 0,
                value: DescriptorValue::UniformBuffer {
                    buffer: &camera_ubo,
                    array_element: frame,
                },
            }]);

            draw_gen_sets.push(set);
        }

        // Create camera sets
        let mut camera_sets = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for frame in 0..FRAMES_IN_FLIGHT {
            let mut set = DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: layouts.camera.clone(),
                    debug_name: Some(format!("{name}_camera_set_{frame}")),
                },
            )
            .unwrap();

            set.update(&[
                DescriptorSetUpdate {
                    binding: CAMERA_UBO_BINDING,
                    array_element: 0,
                    value: DescriptorValue::UniformBuffer {
                        buffer: &camera_ubo,
                        array_element: frame,
                    },
                },
                DescriptorSetUpdate {
                    binding: CAMERA_SHADOW_INFO_BINDING,
                    array_element: 0,
                    value: DescriptorValue::UniformBuffer {
                        buffer: &camera_ubo,
                        array_element: frame,
                    },
                },
            ]);

            camera_sets.push(set);
        }

        // Create skybox sets
        let mut sky_box_sets = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for frame in 0..FRAMES_IN_FLIGHT {
            let mut set = DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: layouts.sky_box.clone(),
                    debug_name: Some(format!("{name}_sky_box_set{frame}")),
                },
            )
            .unwrap();

            set.update(&[DescriptorSetUpdate {
                binding: SKYBOX_CAMERA_BINDING,
                array_element: 0,
                value: DescriptorValue::UniformBuffer {
                    buffer: &camera_ubo,
                    array_element: frame,
                },
            }]);

            sky_box_sets.push(set);
        }

        Self {
            global_sets,
            draw_gen_sets,
            camera_sets,
            sky_box_sets,
            keys: Default::default(),
            camera_ubo,
            clustering,
            input_ids,
            output_ids,
            draw_calls,
            static_input_ids: Vec::default(),
            dynamic_input_ids: Vec::default(),
            static_objects: 0,
            dynamic_objects: 0,
            last_static_draws: 0,
            static_draws: 0,
            dynamic_draws: 0,
        }
    }

    /// Rebinds all draw generation descriptor set values.
    pub fn update_draw_gen_set(
        &mut self,
        global: &GlobalRenderData,
        hzb: Option<&HzbImage>,
        frame: usize,
        use_alternate: bool,
    ) {
        let alternate_frame = (frame * 2) + use_alternate as usize;
        let set = &mut self.draw_gen_sets[frame];
        set.update(&[
            DescriptorSetUpdate {
                binding: DRAW_GEN_DRAW_CALLS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: &self.draw_calls,
                    array_element: alternate_frame,
                },
            },
            DescriptorSetUpdate {
                binding: DRAW_GEN_OBJECT_DATA_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: &global.object_data,
                    array_element: frame,
                },
            },
            DescriptorSetUpdate {
                binding: DRAW_GEN_INPUT_ID_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: &self.input_ids,
                    array_element: frame,
                },
            },
            DescriptorSetUpdate {
                binding: DRAW_GEN_OUTPUT_ID_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: &self.output_ids,
                    array_element: 0,
                },
            },
        ]);

        if let Some(hzb) = hzb {
            let hzb_tex = hzb.texture();
            set.update(&[DescriptorSetUpdate {
                binding: DRAW_GEN_HZB_BINDING,
                array_element: 0,
                value: DescriptorValue::Texture {
                    texture: hzb_tex,
                    array_element: 0,
                    sampler: Sampler {
                        min_filter: Filter::Nearest,
                        mag_filter: Filter::Nearest,
                        mipmap_filter: Filter::Nearest,
                        address_u: SamplerAddressMode::ClampToEdge,
                        address_v: SamplerAddressMode::ClampToEdge,
                        address_w: SamplerAddressMode::ClampToEdge,
                        anisotropy: None,
                        compare: None,
                        min_lod: NotNan::new(0.0).unwrap(),
                        max_lod: None,
                        border_color: None,
                        unnormalize_coords: false,
                    },
                    base_mip: 0,
                    mip_count: hzb_tex.mip_count(),
                },
            }]);
        }
    }

    /// Updates the skybox descriptor set.
    pub fn update_sky_box_set(&mut self, frame: usize, sky_box: &CubeMapInner) {
        let set = &mut self.sky_box_sets[frame];
        set.update(&[DescriptorSetUpdate {
            binding: SKYBOX_CUBE_MAP_BINDING,
            array_element: 0,
            value: DescriptorValue::CubeMap {
                cube_map: &sky_box.cube_map,
                array_element: 0,
                sampler: SKY_BOX_SAMPLER,
                base_mip: 0,
                mip_count: sky_box.cube_map.mip_count(),
            },
        }]);
    }

    /// Rebinds all global descriptor set values.
    pub fn update_global_set(
        &mut self,
        global: &GlobalRenderData,
        lighting: &Lighting,
        ibl: &CameraIbl,
        cube_maps: &ResourceAllocator<CubeMapInner>,
        frame: usize,
    ) {
        let set = &mut self.global_sets[frame];
        set.update(&[
            DescriptorSetUpdate {
                binding: GLOBAL_OBJECT_DATA_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: &global.object_data,
                    array_element: frame,
                },
            },
            DescriptorSetUpdate {
                binding: GLOBAL_OBJECT_ID_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: &self.output_ids,
                    array_element: 0,
                },
            },
            DescriptorSetUpdate {
                binding: GLOBAL_LIGHTS_BINDING,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer: &global.lights,
                    array_element: frame,
                },
            },
            DescriptorSetUpdate {
                binding: GLOBAL_LIGHTING_BINDING,
                array_element: 0,
                value: DescriptorValue::UniformBuffer {
                    buffer: &lighting.ubo,
                    array_element: frame,
                },
            },
            DescriptorSetUpdate {
                binding: GLOBAL_BRDF_LUT_BINDING,
                array_element: 0,
                value: DescriptorValue::Texture {
                    texture: &global.brdf_lut,
                    array_element: 0,
                    sampler: BRDF_LUT_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            },
            DescriptorSetUpdate {
                binding: GLOBAL_DIFFUSE_IRRADIANCE_MAP_BINDING,
                array_element: 0,
                value: DescriptorValue::CubeMap {
                    cube_map: match &ibl.diffuse_irradiance {
                        Some(di) => &cube_maps.get(di.id).unwrap().cube_map,
                        None => &global.white_cube_map,
                    },
                    array_element: 0,
                    sampler: SKY_BOX_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            },
            match &ibl.prefiltered_environment {
                Some(di) => {
                    let cube_map = cube_maps.get(di.id).unwrap();
                    let (base_mip, mip_count) = cube_map.loaded_mips();
                    DescriptorSetUpdate {
                        binding: GLOBAL_PREFILTERED_ENV_MAP_BINDING,
                        array_element: 0,
                        value: DescriptorValue::CubeMap {
                            cube_map: &cube_map.cube_map,
                            array_element: 0,
                            sampler: SKY_BOX_SAMPLER,
                            base_mip: base_mip as usize,
                            mip_count: mip_count as usize,
                        },
                    }
                }
                None => DescriptorSetUpdate {
                    binding: GLOBAL_PREFILTERED_ENV_MAP_BINDING,
                    array_element: 0,
                    value: DescriptorValue::CubeMap {
                        cube_map: &global.white_cube_map,
                        array_element: 0,
                        sampler: SKY_BOX_SAMPLER,
                        base_mip: 0,
                        mip_count: 1,
                    },
                },
            },
        ]);
    }

    /// Updates the camera UBO.
    #[inline]
    pub fn update_camera_ubo(&mut self, frame: usize, data: CameraUbo) {
        let mut view = self.camera_ubo.write(frame).unwrap();
        bytemuck::cast_slice_mut::<_, CameraUbo>(view.deref_mut())[0] = data;
    }

    /// Updates the camera set with the AO texture.
    pub fn update_camera_ao(&mut self, frame: usize, ao_tex: &Texture) {
        self.camera_sets[frame].update(&[DescriptorSetUpdate {
            binding: CAMERA_AO_BINDING,
            array_element: 0,
            value: DescriptorValue::Texture {
                texture: ao_tex,
                array_element: 0,
                sampler: AO_SAMPLER,
                base_mip: 0,
                mip_count: 1,
            },
        }])
    }

    /// Updates the camera set with shadow info.
    pub fn update_camera_with_shadows(
        &mut self,
        frame: usize,
        global: &GlobalRenderData,
        shadows: Option<&Shadows>,
    ) {
        match shadows {
            Some(shadows) => {
                // Bind the info UBO
                self.camera_sets[frame].update(&[DescriptorSetUpdate {
                    binding: CAMERA_SHADOW_INFO_BINDING,
                    array_element: 0,
                    value: DescriptorValue::UniformBuffer {
                        buffer: &shadows.ubo,
                        array_element: frame,
                    },
                }]);

                // Bind available cascades
                for (i, cascade) in shadows.cascades.iter().enumerate() {
                    self.camera_sets[frame].update(&[DescriptorSetUpdate {
                        binding: CAMERA_SHADOW_MAPS_BINDING,
                        array_element: i,
                        value: DescriptorValue::Texture {
                            texture: &cascade.map,
                            array_element: 0,
                            sampler: SHADOW_SAMPLER,
                            base_mip: 0,
                            mip_count: 1,
                        },
                    }]);
                }

                // Fill the rest with empty cascades
                for i in shadows.cascades.len()..MAX_SHADOW_CASCADES {
                    self.camera_sets[frame].update(&[DescriptorSetUpdate {
                        binding: CAMERA_SHADOW_MAPS_BINDING,
                        array_element: i,
                        value: DescriptorValue::Texture {
                            texture: &global.empty_shadow,
                            array_element: frame,
                            sampler: SHADOW_SAMPLER,
                            base_mip: 0,
                            mip_count: 1,
                        },
                    }]);
                }
            }
            None => {
                // Bind empty cascades
                for i in 0..MAX_SHADOW_CASCADES {
                    self.camera_sets[frame].update(&[DescriptorSetUpdate {
                        binding: CAMERA_SHADOW_MAPS_BINDING,
                        array_element: i,
                        value: DescriptorValue::Texture {
                            texture: &global.empty_shadow,
                            array_element: frame,
                            sampler: SHADOW_SAMPLER,
                            base_mip: 0,
                            mip_count: 1,
                        },
                    }]);
                }
            }
        }
    }

    /// Prepares the input ID objects based on the provided query. Also generates draw keys to use
    /// when generating draw calls.
    ///
    /// Returns `true` if the ID buffer was expanded.
    pub fn prepare_input_ids(
        &mut self,
        frame: usize,
        layers: RenderLayer,
        queries: &Queries<Everything>,
        static_geometry: &StaticGeometryInner,
    ) -> bool {
        // Record last static draw count
        self.last_static_draws = self.static_draws;

        // Reset keys for the frame
        let keys = &mut self.keys[frame];

        // State tracking:

        // The combined total number of objects
        let mut object_count = 0;

        // Offset within the global object data buffer
        let mut data_offset = 0;

        // Write in static geometry if it's dirty
        if static_geometry.dirty[frame] {
            // Reset state
            keys.clear();
            self.static_input_ids.clear();
            self.static_draws = 0;
            self.static_objects = 0;

            for key in &static_geometry.sorted_keys {
                let batch = static_geometry.batches.get(key).unwrap();

                // Skip this batch if we don't have compatible layers
                if batch.renderable.layers & layers == RenderLayer::empty() {
                    data_offset += batch.ids.len();
                    continue;
                }

                // Add in the key
                keys.push((*key, batch.ids.len()));

                // Add in the input IDs
                for _ in 0..batch.ids.len() {
                    self.static_input_ids.push(InputObjectId {
                        data_idx: data_offset as u32,
                        draw_idx: [self.static_draws as u32, 0],
                        _dummy: 0,
                    });
                    self.static_objects += 1;
                    object_count += 1;
                    data_offset += 1;
                }

                self.static_draws += 1;
            }
        }
        // Otherwise, just move the offset
        else {
            keys.truncate(self.static_draws);
            object_count = self.static_objects;
            data_offset = static_geometry.len;
        }

        // This loops over all dynamic objects that can possibly be rendered and filters out
        // objects that are disabled and/or missing a compatible render layer.
        self.dynamic_objects = 0;
        self.dynamic_input_ids.clear();
        for (data_idx, (_, (renderable, _), _)) in queries
            .make::<(Entity, (Read<Renderable>, Read<Model>), Read<Disabled>)>()
            .into_iter()
            .enumerate()
            .filter(|(_, (_, (renderable, _), disabled))| {
                // Filter out objects that don't share at least one layer with us or that are
                // disabled
                (renderable.layers & layers != RenderLayer::empty()) && disabled.is_none()
            })
        {
            object_count += 1;
            self.dynamic_objects += 1;

            // NOTE: Instead of writing the batch index here, we write the draw key so that we can
            // sort the object IDs here in place and then later determine the batch index. This is
            // fine as long as the size of object used for `batch_idx` is the same as the size used
            // for `DrawKey`.
            self.dynamic_input_ids.push(InputObjectId {
                data_idx: (data_offset + data_idx) as u32,
                draw_idx: bytemuck::cast(make_draw_key(&renderable.material, &renderable.mesh)),
                _dummy: 0,
            });
        }

        // Sort the object IDs based on the draw key we wrote previously
        self.dynamic_input_ids
            .sort_unstable_by_key(|id| bytemuck::cast::<_, DrawKey>(id.draw_idx));

        // Convert the draw keys into draw indices
        let mut cur_key = DrawKey::MAX;
        self.dynamic_draws = 0;
        for id in &mut self.dynamic_input_ids {
            // Different draw key = new draw call
            let new_key = bytemuck::cast(id.draw_idx);
            if new_key != cur_key {
                cur_key = new_key;
                keys.push((cur_key, 0));
                self.dynamic_draws += 1;
            }

            // Update the draw count
            let draw_idx = keys.len() - 1;
            keys[draw_idx].1 += 1;
            id.draw_idx[0] = draw_idx as u32;
            id.draw_idx[1] = draw_idx as u32;
        }

        // Expand buffer if needed
        let new_size = object_count * std::mem::size_of::<InputObjectId>();
        let expanded = match Buffer::expand(
            &self.input_ids,
            new_size as u64,
            !static_geometry.dirty[frame],
        ) {
            Some(mut new_buffer) => {
                std::mem::swap(&mut self.input_ids, &mut new_buffer);
                true
            }
            None => false,
        };

        // Write in static ids if needed
        let mut id_view = self.input_ids.write(frame).unwrap();
        let id_slice = bytemuck::cast_slice_mut::<_, InputObjectId>(id_view.deref_mut());

        if true {
            // static_geometry.dirty[frame] {
            id_slice[0..self.static_objects].copy_from_slice(&self.static_input_ids);
        }

        // Write into the buffer
        id_slice[self.static_objects..object_count].copy_from_slice(&self.dynamic_input_ids);

        // Expand the output buffer if needed
        if self.output_ids.size() as usize / std::mem::size_of::<OutputObjectId>() < object_count {
            self.output_ids = Buffer::expand(
                &self.output_ids,
                (std::mem::size_of::<OutputObjectId>() * object_count) as u64,
                false,
            )
            .unwrap();
        }

        expanded
    }

    /// Prepares the draw calls for generation based on the keys generated in `prepare_input_ids`.
    ///
    /// Returns `true` if the draw call buffer was expanded.
    pub fn prepare_draw_calls(
        &mut self,
        frame: usize,
        use_alternate: bool,
        factory: &Factory,
    ) -> bool {
        let meshes = factory.0.meshes.lock().unwrap();

        // Expand the draw calls buffer if needed
        // NOTE: Preserve is required for static draw calls.
        let expanded = match Buffer::expand(
            &self.draw_calls,
            (self.keys[frame].len() * std::mem::size_of::<DrawCall>()) as u64,
            true,
        ) {
            Some(buffer) => {
                self.draw_calls = buffer;
                true
            }
            None => false,
        };

        let alternate_frame = (frame * 2) + use_alternate as usize;
        let mut cur_offset = 0;
        let mut draw_call_view = self.draw_calls.write(alternate_frame).unwrap();
        let draw_call_slice = bytemuck::cast_slice_mut::<_, DrawCall>(draw_call_view.deref_mut());

        for (i, (key, draw_count)) in self.keys[frame].iter().enumerate() {
            // Grab the mesh used by this draw
            let (_, _, mesh, _) = from_draw_key(*key);
            let mesh = meshes.get(mesh).unwrap();

            // Write in the draw call
            draw_call_slice[i] = DrawCall {
                indirect: DrawIndexedIndirect {
                    index_count: mesh.index_count as u32,
                    instance_count: 0,
                    first_index: mesh.index_block.base(),
                    vertex_offset: mesh.vertex_block.base() as i32,
                    first_instance: cur_offset as u32,
                },
                bounds: mesh.bounds,
            };
            cur_offset += draw_count;
        }

        expanded
    }

    /// Dispatches a compute pass to generate the draw calls.
    pub fn generate_draw_calls<'a>(
        &'a self,
        frame: usize,
        global: &GlobalRenderData,
        use_hzb_culling: bool,
        render_area: Vec2,
        commands: &mut CommandBuffer<'a>,
    ) {
        // Perform draw generation with the compute shader
        commands.compute_pass(|pass| {
            pass.bind_pipeline(if use_hzb_culling {
                global.draw_gen_pipeline.clone()
            } else {
                global.draw_gen_no_hzb_pipeline.clone()
            });
            pass.bind_sets(0, vec![&self.draw_gen_sets[frame]]);

            // Determine the number of groups to dispatch
            let object_count = self.static_objects + self.dynamic_objects;
            let group_count = if object_count as u32 % DRAW_GEN_WORKGROUP_SIZE != 0 {
                (object_count as u32 / DRAW_GEN_WORKGROUP_SIZE) + 1
            } else {
                object_count as u32 / DRAW_GEN_WORKGROUP_SIZE
            }
            .max(1);
            let push_constants = [DrawGenPushConstants {
                render_area,
                object_count: object_count as u32,
            }];
            pass.push_constants(bytemuck::cast_slice(&push_constants));

            pass.dispatch(group_count, 1, 1);
        });
    }

    /// Performs actual rendering
    pub fn render<'a, 'b>(&'a self, frame: usize, use_alternate: bool, args: RenderArgs<'a, 'b>) {
        let alternate_frame = (frame * 2) + use_alternate as usize;

        // Draw in the skybox if requested
        if args.draw_sky_box {
            args.pass
                .bind_pipeline(args.global.sky_box_pipeline.clone());
            args.pass.bind_sets(0, vec![&self.sky_box_sets[frame]]);
            args.pass.draw(36, 1, 0, 0);
        }

        // State:

        // These values keep track of the active resource type
        let mut last_material = ResourceId(usize::MAX);
        let mut last_mesh = ResourceId(usize::MAX);
        let mut last_mesh_vl = None;
        let mut last_mat_vl = None;
        let mut last_ubo_size = u64::MAX;

        // The number of draws to perform (if needed) and the offset within the draw buffer to
        // pull draws from
        let mut draw_count = 0;
        let mut draw_offset = args.draw_offset as u64;

        // Bind our index buffer
        args.pass.bind_index_buffer(
            args.mesh_buffers.get_index_buffer().buffer(),
            0,
            0,
            IndexType::U32,
        );

        // Loop over every draw call (key)
        for (i, (key, _)) in self.keys[frame][..(args.draw_offset + args.draw_count)]
            .iter()
            .enumerate()
            .skip(args.draw_offset)
        {
            let (material_id, vertex_layout, mesh_id, _) = match &args.material_override {
                Some(material) => {
                    let (_, vertex_layout, mesh_id, unused) = from_draw_key(*key);
                    (material.id, vertex_layout, mesh_id, unused)
                }
                None => from_draw_key(*key),
            };

            from_draw_key(*key);
            let mat_vertex_layout = args.materials.get(material_id).unwrap().layout;

            // Determine what needs a rebind
            let new_material = last_material != material_id;
            let mut new_vb = match &mut last_mesh_vl {
                Some(old_layout) => {
                    if *old_layout != vertex_layout {
                        *old_layout = vertex_layout;
                        true
                    } else {
                        false
                    }
                }
                None => true,
            };
            new_vb |= match &mut last_mat_vl {
                Some(old_layout) => {
                    if *old_layout != mat_vertex_layout {
                        *old_layout = mat_vertex_layout;
                        true
                    } else {
                        false
                    }
                }
                None => true,
            };
            let new_mesh_not_ready = if last_mesh != mesh_id {
                // Check if the new mesh we have just observed is not ready
                !args.meshes.get(mesh_id).unwrap().ready
            } else {
                false
            };

            // If anything needs a rebind, check if we should draw
            if new_material || new_vb || new_mesh_not_ready {
                if draw_count > 0 {
                    args.pass.draw_indexed_indirect(
                        &self.draw_calls,
                        alternate_frame,
                        draw_offset * std::mem::size_of::<DrawCall>() as u64,
                        draw_count,
                        std::mem::size_of::<DrawCall>() as u64,
                    );
                }

                // If the new mesh we see is not ready, we must skip it
                draw_count = 0;
                if new_mesh_not_ready {
                    draw_offset = i as u64 + 1;
                    continue;
                }
                // Otherwise, set the offset to the mesh
                else {
                    last_mesh = mesh_id;
                    draw_offset = i as u64;
                }
            }

            // Rebind material
            let material = args.materials.get(material_id).unwrap();
            if new_material {
                args.pass
                    .bind_pipeline(material.pipelines.get(args.pipeline_ty).clone());

                // If the last material as the invalid ID, then we must also bind our global sets
                if last_material == ResourceId(usize::MAX) {
                    args.pass.bind_sets(
                        0,
                        vec![
                            &self.global_sets[frame],
                            &self.camera_sets[frame],
                            args.texture_sets.set(frame),
                        ],
                    );
                }

                // A new material possibly means a new size of material instance data
                if material.data_size != last_ubo_size
                    && (material.data_size != 0 || material.texture_count != 0)
                {
                    args.pass.bind_sets(
                        3,
                        vec![args
                            .material_buffers
                            .get_set(material.data_size, frame)
                            .unwrap()],
                    );
                    last_ubo_size = material.data_size;
                }

                last_material = material_id;
            }

            // Rebind vertex buffers
            if new_vb {
                last_mesh_vl = Some(vertex_layout);
                last_mat_vl = Some(mat_vertex_layout);
                let vbuffer = args.mesh_buffers.get_vertex_buffer(vertex_layout).unwrap();
                vbuffer.bind(args.pass, mat_vertex_layout);
            }

            draw_count += 1;
        }

        // Perform a final draw if required
        if draw_count > 0 {
            args.pass.draw_indexed_indirect(
                &self.draw_calls,
                alternate_frame,
                draw_offset * std::mem::size_of::<DrawCall>() as u64,
                draw_count,
                std::mem::size_of::<DrawCall>() as u64,
            );
        }
    }
}

#[inline(always)]
pub(crate) fn make_draw_key(instance: &MaterialInstance, mesh: &Mesh) -> DrawKey {
    // [Material][Vertex Layout][Mesh   ][MaterialInstance]
    // [ 24 bits][       8 bits][16 bits][         16 bits]

    // Upper 10 bits are pipeline. Middle 11 are material. Bottom 11 are mesh.
    let mut out = 0;
    out |= (instance.material.id.0 as u64 & ((1 << 24) - 1)) << 40;
    out |= (mesh.layout.bits() as u64 & ((1 << 8) - 1)) << 32;
    out |= (mesh.id.0 as u64 & ((1 << 16) - 1)) << 16;
    out |= instance.id.0 as u64 & ((1 << 16) - 1);
    out
}

/// Material, vertex layout, mesh, and material instance ids in that order.
#[inline(always)]
pub(crate) fn from_draw_key(key: DrawKey) -> (ResourceId, VertexLayout, ResourceId, ResourceId) {
    (
        ResourceId((key >> 40) as usize & ((1 << 24) - 1)),
        unsafe { VertexLayout::from_bits_unchecked(((key >> 32) & ((1 << 8) - 1)) as u8) },
        ResourceId((key >> 16) as usize & ((1 << 16) - 1)),
        ResourceId(key as usize & ((1 << 16) - 1)),
    )
}
