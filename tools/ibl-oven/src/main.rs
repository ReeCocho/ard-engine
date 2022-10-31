use std::{
    ops::Shr,
    path::{Path, PathBuf},
};

use ard_formats::cube_map::CubeMapHeader;
use ard_math::*;
use ard_pal::{
    backend::{VulkanBackend, VulkanBackendCreateInfo},
    prelude::*,
};
use bytemuck::{Pod, Zeroable};
use clap::Parser;
use ordered_float::NotNan;
use winit::{dpi::PhysicalSize, event_loop::EventLoop, window::WindowBuilder};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the HDR image to bake.
    #[arg(short, long)]
    path: PathBuf,
    /// Compress resulting textures.
    #[arg(long, default_value_t = false)]
    compress_textures: bool,
    /// Output resolution of the generated cube maps.
    #[arg(long, default_value_t = 512)]
    resolution: u32,
}

fn main() {
    let args = Args::parse();

    // Make an empty window for the backend
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("ibl-oven")
        .with_inner_size(PhysicalSize::new(128, 128))
        .with_visible(false)
        .build(&event_loop)
        .unwrap();

    // Initialize Pal
    println!("Initializing graphics...");
    let vk = VulkanBackend::new(VulkanBackendCreateInfo {
        app_name: String::from("ibl-oven"),
        engine_name: String::from("ard-engine"),
        window: &window,
        debug: true,
    })
    .unwrap();

    let pal = Context::new(vk);

    // Load in the HDR image
    let hdr_texture = load_image(&pal, &args);

    // Generate a cube map from the image
    let cube_map = to_cube_map(&pal, &args, &hdr_texture);

    // Compute diffuse irradiance cube map
    let diffuse_irradiance_cube_map = create_diffuse_irradiance(&pal, &cube_map);

    // Compute prefiltered environment map
    let prefiltered_env_map = create_prefiltered_env_map(&pal, &cube_map);

    // Save the cube maps to disk
    println!("Saving to disk...");
    let mut buffers = cube_maps_to_buffers(
        &pal,
        &[
            &cube_map,
            &diffuse_irradiance_cube_map,
            &prefiltered_env_map,
        ],
    );
    let mut prefiltered_env_buffer = buffers.pop().unwrap();
    let mut diffuse_irradiance_buffer = buffers.pop().unwrap();
    let mut cube_map_buffer = buffers.pop().unwrap();

    rayon::join(
        || {
            save_cube_map_buffer(
                &args.path,
                args.compress_textures,
                &cube_map,
                &mut cube_map_buffer,
                "",
            )
        },
        || {
            rayon::join(
                || {
                    save_cube_map_buffer(
                        &args.path,
                        false,
                        &diffuse_irradiance_cube_map,
                        &mut diffuse_irradiance_buffer,
                        "dir",
                    )
                },
                || {
                    save_cube_map_buffer(
                        &args.path,
                        false,
                        &prefiltered_env_map,
                        &mut prefiltered_env_buffer,
                        "pem",
                    )
                },
            )
        },
    );
}

#[derive(Copy, Clone)]
#[allow(dead_code)]
struct PushConstants {
    vp: Mat4,
    roughness: f32,
}

unsafe impl Pod for PushConstants {}
unsafe impl Zeroable for PushConstants {}

// Loads in the HDR texture
fn load_image(pal: &Context, args: &Args) -> Texture {
    assert!(
        args.path.extension().unwrap().eq("hdr"),
        "Invalid image format. Must be HDR."
    );
    println!("Loading image...");
    let bytes = std::fs::read(&args.path).unwrap();
    let image_data = image::codecs::hdr::HdrDecoder::new(bytes.as_slice()).unwrap();

    let width = image_data.metadata().width;
    let height = image_data.metadata().height;
    let hdr_rgb = image_data.read_image_hdr().unwrap();

    // Convert to RGBA
    let mut hdr_rgba = Vec::with_capacity(hdr_rgb.len() * 4);
    for texel in hdr_rgb {
        hdr_rgba.push(texel.0[0]);
        hdr_rgba.push(texel.0[1]);
        hdr_rgba.push(texel.0[2]);
        hdr_rgba.push(0.0);
    }

    let hdr_staging = Buffer::new_staging(
        pal.clone(),
        Some(String::from("hdr_staging")),
        bytemuck::cast_slice(&hdr_rgba),
    )
    .unwrap();

    let hdr_texture = Texture::new(
        pal.clone(),
        TextureCreateInfo {
            format: TextureFormat::Rgba32SFloat,
            ty: TextureType::Type2D,
            width,
            height,
            depth: 1,
            array_elements: 1,
            mip_levels: 1,
            texture_usage: TextureUsage::SAMPLED | TextureUsage::TRANSFER_DST,
            memory_usage: MemoryUsage::GpuOnly,
            debug_name: Some(String::from("hdr_texture")),
        },
    )
    .unwrap();

    let mut commands = pal.transfer().command_buffer();
    commands.copy_buffer_to_texture(
        &hdr_texture,
        &hdr_staging,
        BufferTextureCopy {
            buffer_offset: 0,
            buffer_row_length: 0,
            buffer_image_height: 0,
            buffer_array_element: 0,
            texture_offset: (0, 0, 0),
            texture_extent: (width, height, 1),
            texture_mip_level: 0,
            texture_array_element: 0,
        },
    );
    pal.transfer().submit(Some("hdr_texture_Upload"), commands);

    hdr_texture
}

// Converts the HDR texutre into a cube map.
fn to_cube_map(pal: &Context, args: &Args, hdr_texture: &Texture) -> CubeMap {
    println!("Generating cube map...");

    let cubemap = CubeMap::new(
        pal.clone(),
        CubeMapCreateInfo {
            format: TextureFormat::Rgba16SFloat,
            size: args.resolution,
            array_elements: 1,
            mip_levels: 1,
            texture_usage: TextureUsage::COLOR_ATTACHMENT
                | TextureUsage::SAMPLED
                | TextureUsage::TRANSFER_SRC,
            memory_usage: MemoryUsage::GpuOnly,
            debug_name: Some(String::from("cubemap")),
        },
    )
    .unwrap();

    let layout = DescriptorSetLayout::new(
        pal.clone(),
        DescriptorSetLayoutCreateInfo {
            bindings: vec![DescriptorBinding {
                binding: 0,
                ty: DescriptorType::Texture,
                count: 1,
                stage: ShaderStage::Fragment,
            }],
        },
    )
    .unwrap();

    let mut descriptor_set = DescriptorSet::new(
        pal.clone(),
        DescriptorSetCreateInfo {
            layout: layout.clone(),
            debug_name: Some(String::from("cube_map_gen_set")),
        },
    )
    .unwrap();

    descriptor_set.update(&[DescriptorSetUpdate {
        binding: 0,
        array_element: 0,
        value: DescriptorValue::Texture {
            texture: hdr_texture,
            array_element: 0,
            sampler: Sampler {
                min_filter: Filter::Linear,
                mag_filter: Filter::Linear,
                mipmap_filter: Filter::Linear,
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
            mip_count: 1,
        },
    }]);

    let vertex = Shader::new(
        pal.clone(),
        ShaderCreateInfo {
            code: include_bytes!("./er_to_cube.vert.spv"),
            debug_name: Some(String::from("eq_to_cube.vert")),
        },
    )
    .unwrap();

    let fragment = Shader::new(
        pal.clone(),
        ShaderCreateInfo {
            code: include_bytes!("./er_to_cube.frag.spv"),
            debug_name: Some(String::from("eq_to_cube.frag")),
        },
    )
    .unwrap();

    let pipeline = GraphicsPipeline::new(
        pal.clone(),
        GraphicsPipelineCreateInfo {
            stages: ShaderStages {
                vertex,
                fragment: Some(fragment),
            },
            layouts: vec![layout],
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
            depth_stencil: None,
            color_blend: Some(ColorBlendState {
                attachments: vec![ColorBlendAttachment {
                    write_mask: ColorComponents::ALL,
                    blend: false,
                    ..Default::default()
                }],
            }),
            push_constants_size: Some(std::mem::size_of::<PushConstants>() as u32),
            debug_name: Some(String::from("eq_to_cube")),
        },
    )
    .unwrap();

    let mut commands = pal.main().command_buffer();
    draw_cube_faces(&mut commands, &pipeline, &descriptor_set, &cubemap, 0, 0.0);
    pal.main().submit(Some("cube_map_gen"), commands);

    cubemap
}

// Creates the diffuse irradiance cube map.
fn create_diffuse_irradiance(pal: &Context, cube_map: &CubeMap) -> CubeMap {
    println!("Generating diffuse irradiance cube map...");

    // Diffuse irradiance map never needs to be high resolution, so this constant should be good
    // for all use cases (watch me be wrong OMEGALUL).
    const DIFFUSE_IRRADIANCE_RESOLUTION: u32 = 32;
    let diffuse_irradiance = CubeMap::new(
        pal.clone(),
        CubeMapCreateInfo {
            format: TextureFormat::Rgba16SFloat,
            size: DIFFUSE_IRRADIANCE_RESOLUTION,
            array_elements: 1,
            mip_levels: 1,
            texture_usage: TextureUsage::COLOR_ATTACHMENT | TextureUsage::TRANSFER_SRC,
            memory_usage: MemoryUsage::GpuOnly,
            debug_name: Some(String::from("diffuse_irradiance_map")),
        },
    )
    .unwrap();

    let layout = DescriptorSetLayout::new(
        pal.clone(),
        DescriptorSetLayoutCreateInfo {
            bindings: vec![DescriptorBinding {
                binding: 0,
                ty: DescriptorType::CubeMap,
                count: 1,
                stage: ShaderStage::Fragment,
            }],
        },
    )
    .unwrap();

    let mut descriptor_set = DescriptorSet::new(
        pal.clone(),
        DescriptorSetCreateInfo {
            layout: layout.clone(),
            debug_name: Some(String::from("diffuse_irradiance_gen_set")),
        },
    )
    .unwrap();

    descriptor_set.update(&[DescriptorSetUpdate {
        binding: 0,
        array_element: 0,
        value: DescriptorValue::CubeMap {
            cube_map,
            array_element: 0,
            sampler: Sampler {
                min_filter: Filter::Linear,
                mag_filter: Filter::Linear,
                mipmap_filter: Filter::Linear,
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
            mip_count: 1,
        },
    }]);

    let vertex = Shader::new(
        pal.clone(),
        ShaderCreateInfo {
            code: include_bytes!("./er_to_cube.vert.spv"),
            debug_name: Some(String::from("eq_to_cube.vert")),
        },
    )
    .unwrap();

    let fragment = Shader::new(
        pal.clone(),
        ShaderCreateInfo {
            code: include_bytes!("./diffuse_irradiance.frag.spv"),
            debug_name: Some(String::from("diffuse_irradiance.frag")),
        },
    )
    .unwrap();

    let pipeline = GraphicsPipeline::new(
        pal.clone(),
        GraphicsPipelineCreateInfo {
            stages: ShaderStages {
                vertex,
                fragment: Some(fragment),
            },
            layouts: vec![layout],
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
            depth_stencil: None,
            color_blend: Some(ColorBlendState {
                attachments: vec![ColorBlendAttachment {
                    write_mask: ColorComponents::ALL,
                    blend: false,
                    ..Default::default()
                }],
            }),
            push_constants_size: Some(std::mem::size_of::<PushConstants>() as u32),
            debug_name: Some(String::from("diffuse_irradiance_gen")),
        },
    )
    .unwrap();

    let mut commands = pal.main().command_buffer();
    draw_cube_faces(
        &mut commands,
        &pipeline,
        &descriptor_set,
        &diffuse_irradiance,
        0,
        0.0,
    );
    pal.main().submit(Some("diffuse_irradiance_gen"), commands);

    diffuse_irradiance
}

// Creates the prefiltered environment map.
fn create_prefiltered_env_map(pal: &Context, cube_map: &CubeMap) -> CubeMap {
    println!("Generating prefiltered environment map...");

    const PF_ENV_MAP_RESOLUTION: u32 = 128;
    const PF_ENV_MIP_COUNT: usize = 5;

    let pf_env = CubeMap::new(
        pal.clone(),
        CubeMapCreateInfo {
            format: TextureFormat::Rgba16SFloat,
            size: PF_ENV_MAP_RESOLUTION,
            array_elements: 1,
            mip_levels: PF_ENV_MIP_COUNT,
            texture_usage: TextureUsage::COLOR_ATTACHMENT | TextureUsage::TRANSFER_SRC,
            memory_usage: MemoryUsage::GpuOnly,
            debug_name: Some(String::from("prefiltered_environment_map")),
        },
    )
    .unwrap();

    let layout = DescriptorSetLayout::new(
        pal.clone(),
        DescriptorSetLayoutCreateInfo {
            bindings: vec![DescriptorBinding {
                binding: 0,
                ty: DescriptorType::CubeMap,
                count: 1,
                stage: ShaderStage::Fragment,
            }],
        },
    )
    .unwrap();

    let mut descriptor_set = DescriptorSet::new(
        pal.clone(),
        DescriptorSetCreateInfo {
            layout: layout.clone(),
            debug_name: Some(String::from("prefiltered_env_map_gen_set")),
        },
    )
    .unwrap();

    descriptor_set.update(&[DescriptorSetUpdate {
        binding: 0,
        array_element: 0,
        value: DescriptorValue::CubeMap {
            cube_map,
            array_element: 0,
            sampler: Sampler {
                min_filter: Filter::Linear,
                mag_filter: Filter::Linear,
                mipmap_filter: Filter::Linear,
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
            mip_count: 1,
        },
    }]);

    let vertex = Shader::new(
        pal.clone(),
        ShaderCreateInfo {
            code: include_bytes!("./er_to_cube.vert.spv"),
            debug_name: Some(String::from("eq_to_cube.vert")),
        },
    )
    .unwrap();

    let fragment = Shader::new(
        pal.clone(),
        ShaderCreateInfo {
            code: include_bytes!("./prefiltered_env_map.frag.spv"),
            debug_name: Some(String::from("prefiltered_env_map.frag")),
        },
    )
    .unwrap();

    let pipeline = GraphicsPipeline::new(
        pal.clone(),
        GraphicsPipelineCreateInfo {
            stages: ShaderStages {
                vertex,
                fragment: Some(fragment),
            },
            layouts: vec![layout],
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
            depth_stencil: None,
            color_blend: Some(ColorBlendState {
                attachments: vec![ColorBlendAttachment {
                    write_mask: ColorComponents::ALL,
                    blend: false,
                    ..Default::default()
                }],
            }),
            push_constants_size: Some(std::mem::size_of::<PushConstants>() as u32),
            debug_name: Some(String::from("prefiltered_env_gen")),
        },
    )
    .unwrap();

    let mut commands = pal.main().command_buffer();
    for mip in 0..PF_ENV_MIP_COUNT {
        let roughness = mip as f32 / (PF_ENV_MIP_COUNT - 1) as f32;
        draw_cube_faces(
            &mut commands,
            &pipeline,
            &descriptor_set,
            &pf_env,
            mip,
            roughness,
        );
    }
    pal.main().submit(Some("diffuse_irradiance_gen"), commands);

    pf_env
}

// Saves cube maps to CPU readable buffers.
fn cube_maps_to_buffers(pal: &Context, cube_maps: &[&CubeMap]) -> Vec<Buffer> {
    let mut buffers = Vec::with_capacity(cube_maps.len());
    let mut commands = pal.transfer().command_buffer();

    // Create buffers
    for cube_map in cube_maps {
        buffers.push(
            Buffer::new(
                pal.clone(),
                BufferCreateInfo {
                    size: cube_map.size(),
                    array_elements: 1,
                    buffer_usage: BufferUsage::TRANSFER_DST,
                    memory_usage: MemoryUsage::GpuToCpu,
                    debug_name: Some(String::from("saved_cube_map")),
                },
            )
            .unwrap(),
        );
    }

    // Save cube maps
    for (i, cube_map) in cube_maps.iter().enumerate() {
        let mut offset = 0;
        for mip in 0..cube_map.mip_count() {
            commands.copy_cube_map_to_buffer(
                &buffers[i],
                cube_map,
                BufferCubeMapCopy {
                    buffer_offset: offset,
                    buffer_array_element: 0,
                    cube_map_mip_level: mip,
                    cube_map_array_element: 0,
                },
            );
            offset += cube_map_mip_size(cube_map.dim(), false, mip as u32);
        }
    }

    pal.transfer().submit(Some("cube_map_save"), commands);
    buffers
}

// Saves the cube map buffer to disk.
fn save_cube_map_buffer(
    root: &Path,
    compress: bool,
    cube_map: &CubeMap,
    buffer: &Buffer,
    ty: &str,
) {
    use rayon::prelude::*;

    let mut path = PathBuf::from(root.parent().unwrap());
    if ty.is_empty() {
        path.push(format!(
            "{}.ard_cube",
            root.file_stem().unwrap().to_str().unwrap(),
        ));
    } else {
        path.push(format!(
            "{}.{}.ard_cube",
            root.file_stem().unwrap().to_str().unwrap(),
            ty
        ));
    }

    std::fs::create_dir_all(&path).unwrap();

    // Save the header
    let header = CubeMapHeader {
        size: cube_map.dim(),
        mip_count: cube_map.mip_count() as u32,
        format: if compress {
            TextureFormat::BC6HUFloat
        } else {
            TextureFormat::Rgba16SFloat
        },
        sampler: ard_formats::texture::Sampler {
            min_filter: Filter::Linear,
            mag_filter: Filter::Linear,
            mipmap_filter: Filter::Linear,
            address_u: SamplerAddressMode::ClampToEdge,
            address_v: SamplerAddressMode::ClampToEdge,
        },
    };

    let mut header_path = PathBuf::from(&path);
    header_path.push("header");

    std::fs::write(&header_path, bincode::serialize(&header).unwrap()).unwrap();

    // Read the entirety of the cube map into CPU memory
    let view = buffer.read(0).unwrap();
    let mut cube_map_data = Vec::with_capacity(view.len());
    cube_map_data.extend_from_slice(&view);

    // Save each mip
    (0..cube_map.mip_count()).into_par_iter().for_each(|mip| {
        // Compute the offset and size of the mip within the main buffer
        let mut offset = 0;
        for i in 0..mip {
            offset += cube_map_mip_size(cube_map.dim(), false, i as u32);
        }
        let size = cube_map_mip_size(cube_map.dim(), false, mip as u32);

        // Save the mip to disk
        let mut mip_path = PathBuf::from(&path);
        mip_path.push(format!("{mip}"));

        // Compress if needed
        if compress {
            let dim = cube_map.dim().shr(mip).max(1);

            // Compress each face in parallel
            let mut east = Vec::default();
            let mut west = Vec::default();
            let mut top = Vec::default();
            let mut bottom = Vec::default();
            let mut north = Vec::default();
            let mut south = Vec::default();
            let face_size = size / 6;
            rayon::scope(|scope| {
                scope.spawn(|_| {
                    let start = offset as usize;
                    let end = start + face_size as usize;
                    let surface = intel_tex_2::RgbaSurface {
                        width: dim,
                        height: dim,
                        stride: dim * 8,
                        data: &cube_map_data[start..end],
                    };
                    east = intel_tex_2::bc6h::compress_blocks(
                        &intel_tex_2::bc6h::very_fast_settings(),
                        &surface,
                    );
                });

                scope.spawn(|_| {
                    let start = (offset + face_size) as usize;
                    let end = start + face_size as usize;
                    let surface = intel_tex_2::RgbaSurface {
                        width: dim,
                        height: dim,
                        stride: dim * 8,
                        data: &cube_map_data[start..end],
                    };
                    west = intel_tex_2::bc6h::compress_blocks(
                        &intel_tex_2::bc6h::very_fast_settings(),
                        &surface,
                    );
                });

                scope.spawn(|_| {
                    let start = (offset + (face_size * 2)) as usize;
                    let end = start + face_size as usize;
                    let surface = intel_tex_2::RgbaSurface {
                        width: dim,
                        height: dim,
                        stride: dim * 8,
                        data: &cube_map_data[start..end],
                    };
                    top = intel_tex_2::bc6h::compress_blocks(
                        &intel_tex_2::bc6h::very_fast_settings(),
                        &surface,
                    );
                });

                scope.spawn(|_| {
                    let start = (offset + (face_size * 3)) as usize;
                    let end = start + face_size as usize;
                    let surface = intel_tex_2::RgbaSurface {
                        width: dim,
                        height: dim,
                        stride: dim * 8,
                        data: &cube_map_data[start..end],
                    };
                    bottom = intel_tex_2::bc6h::compress_blocks(
                        &intel_tex_2::bc6h::very_fast_settings(),
                        &surface,
                    );
                });

                scope.spawn(|_| {
                    let start = (offset + (face_size * 4)) as usize;
                    let end = start + face_size as usize;
                    let surface = intel_tex_2::RgbaSurface {
                        width: dim,
                        height: dim,
                        stride: dim * 8,
                        data: &cube_map_data[start..end],
                    };
                    north = intel_tex_2::bc6h::compress_blocks(
                        &intel_tex_2::bc6h::very_fast_settings(),
                        &surface,
                    );
                });

                scope.spawn(|_| {
                    let start = (offset + (face_size * 5)) as usize;
                    let end = start + face_size as usize;
                    let surface = intel_tex_2::RgbaSurface {
                        width: dim,
                        height: dim,
                        stride: dim * 8,
                        data: &cube_map_data[start..end],
                    };
                    south = intel_tex_2::bc6h::compress_blocks(
                        &intel_tex_2::bc6h::very_fast_settings(),
                        &surface,
                    );
                });
            });

            let mut bytes = Vec::with_capacity(north.len() * 6);
            bytes.extend_from_slice(&east);
            bytes.extend_from_slice(&west);
            bytes.extend_from_slice(&top);
            bytes.extend_from_slice(&bottom);
            bytes.extend_from_slice(&north);
            bytes.extend_from_slice(&south);

            std::fs::write(mip_path, bytes).unwrap();
        } else {
            std::fs::write(
                mip_path,
                &cube_map_data[offset as usize..(offset + size) as usize],
            )
            .unwrap();
        }
    });
}

#[inline(always)]
fn cube_map_mip_size(base_size: u32, compressed: bool, mip_level: u32) -> u64 {
    let dim = base_size.shr(mip_level).max(1);
    // Either BC6H or RGBA16
    let size = if compressed {
        ((dim * dim) as f32 / 16.0).ceil() as u32 * 16
    } else {
        dim * dim * 6 * 8
    } as u64;
    size
}

fn draw_cube_faces<'a, 'b>(
    commands: &'a mut CommandBuffer<'b>,
    pipeline: &'b GraphicsPipeline,
    set: &'b DescriptorSet,
    cube_map: &'b CubeMap,
    mip: usize,
    roughness: f32,
) {
    let push_constants = generate_vps(roughness);

    commands.render_pass(
        RenderPassDescriptor {
            color_attachments: vec![ColorAttachment {
                source: ColorAttachmentSource::CubeMap {
                    cube_map,
                    array_element: 0,
                    face: CubeFace::East,
                    mip_level: mip,
                },
                load_op: LoadOp::DontCare,
                store_op: StoreOp::Store,
            }],
            depth_stencil_attachment: None,
        },
        |pass| {
            pass.bind_pipeline(pipeline.clone());
            pass.bind_sets(0, vec![set]);
            pass.push_constants(bytemuck::cast_slice(&push_constants[0..1]));
            pass.draw(36, 1, 0, 0);
        },
    );

    commands.render_pass(
        RenderPassDescriptor {
            color_attachments: vec![ColorAttachment {
                source: ColorAttachmentSource::CubeMap {
                    cube_map,
                    array_element: 0,
                    face: CubeFace::West,
                    mip_level: mip,
                },
                load_op: LoadOp::DontCare,
                store_op: StoreOp::Store,
            }],
            depth_stencil_attachment: None,
        },
        |pass| {
            pass.bind_pipeline(pipeline.clone());
            pass.bind_sets(0, vec![set]);
            pass.push_constants(bytemuck::cast_slice(&push_constants[1..2]));
            pass.draw(36, 1, 0, 0);
        },
    );

    commands.render_pass(
        RenderPassDescriptor {
            color_attachments: vec![ColorAttachment {
                source: ColorAttachmentSource::CubeMap {
                    cube_map,
                    array_element: 0,
                    face: CubeFace::Top,
                    mip_level: mip,
                },
                load_op: LoadOp::DontCare,
                store_op: StoreOp::Store,
            }],
            depth_stencil_attachment: None,
        },
        |pass| {
            pass.bind_pipeline(pipeline.clone());
            pass.bind_sets(0, vec![set]);
            pass.push_constants(bytemuck::cast_slice(&push_constants[2..3]));
            pass.draw(36, 1, 0, 0);
        },
    );

    commands.render_pass(
        RenderPassDescriptor {
            color_attachments: vec![ColorAttachment {
                source: ColorAttachmentSource::CubeMap {
                    cube_map,
                    array_element: 0,
                    face: CubeFace::Bottom,
                    mip_level: mip,
                },
                load_op: LoadOp::DontCare,
                store_op: StoreOp::Store,
            }],
            depth_stencil_attachment: None,
        },
        |pass| {
            pass.bind_pipeline(pipeline.clone());
            pass.bind_sets(0, vec![set]);
            pass.push_constants(bytemuck::cast_slice(&push_constants[3..4]));
            pass.draw(36, 1, 0, 0);
        },
    );

    commands.render_pass(
        RenderPassDescriptor {
            color_attachments: vec![ColorAttachment {
                source: ColorAttachmentSource::CubeMap {
                    cube_map,
                    array_element: 0,
                    face: CubeFace::North,
                    mip_level: mip,
                },
                load_op: LoadOp::DontCare,
                store_op: StoreOp::Store,
            }],
            depth_stencil_attachment: None,
        },
        |pass| {
            pass.bind_pipeline(pipeline.clone());
            pass.bind_sets(0, vec![set]);
            pass.push_constants(bytemuck::cast_slice(&push_constants[4..5]));
            pass.draw(36, 1, 0, 0);
        },
    );

    commands.render_pass(
        RenderPassDescriptor {
            color_attachments: vec![ColorAttachment {
                source: ColorAttachmentSource::CubeMap {
                    cube_map,
                    array_element: 0,
                    face: CubeFace::South,
                    mip_level: mip,
                },
                load_op: LoadOp::DontCare,
                store_op: StoreOp::Store,
            }],
            depth_stencil_attachment: None,
        },
        |pass| {
            pass.bind_pipeline(pipeline.clone());
            pass.bind_sets(0, vec![set]);
            pass.push_constants(bytemuck::cast_slice(&push_constants[5..6]));
            pass.draw(36, 1, 0, 0);
        },
    );
}

fn generate_vps(roughness: f32) -> [PushConstants; 6] {
    let perspective = Mat4::perspective_lh(std::f32::consts::FRAC_PI_2, 1.0, 0.1, 10.0);
    [
        // East
        PushConstants {
            vp: perspective * Mat4::look_at_lh(Vec3::ZERO, Vec3::X, Vec3::Y),
            roughness,
        },
        // West
        PushConstants {
            vp: perspective * Mat4::look_at_lh(Vec3::ZERO, -Vec3::X, Vec3::Y),
            roughness,
        },
        // Top
        PushConstants {
            vp: perspective * Mat4::look_at_lh(Vec3::ZERO, Vec3::Y, -Vec3::Z),
            roughness,
        },
        // Bottom
        PushConstants {
            vp: perspective * Mat4::look_at_lh(Vec3::ZERO, -Vec3::Y, Vec3::Z),
            roughness,
        },
        // North
        PushConstants {
            vp: perspective * Mat4::look_at_lh(Vec3::ZERO, Vec3::Z, Vec3::Y),
            roughness,
        },
        // South
        PushConstants {
            vp: perspective * Mat4::look_at_lh(Vec3::ZERO, -Vec3::Z, Vec3::Y),
            roughness,
        },
    ]
}
