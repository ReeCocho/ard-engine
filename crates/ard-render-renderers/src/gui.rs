use ard_log::warn;
use ard_math::{UVec2, Vec2};
use ard_pal::prelude::*;
use ard_render_base::{ecs::Frame, FRAMES_IN_FLIGHT};
use ard_render_gui::GuiRunOutput;
use ard_render_si::{bindings::*, types::*};
use ordered_float::NotNan;

const DEFAULT_VB_SIZE: u64 = 256;
const DEFAULT_IB_SIZE: u64 = 256;

const FONT_SAMPLER: Sampler = Sampler {
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

pub struct GuiDrawPrepare<'a> {
    pub frame: Frame,
    pub canvas_size: (u32, u32),
    pub gui_output: &'a mut GuiRunOutput,
}

pub struct GuiRenderer {
    ctx: Context,
    font_texture: Texture,
    vertex_buffer: Buffer,
    index_buffer: Buffer,
    font_pipeline: GraphicsPipeline,
    sets: [DescriptorSet; FRAMES_IN_FLIGHT],
    draw_calls: Vec<DrawCall>,
    texture_deltas: Vec<TextureDelta>,
}

struct DrawCall {
    vertex_offset: isize,
    index_offset: usize,
    index_count: usize,
    scissor: Scissor,
    _texture_id: u32,
}

struct TextureDelta {
    /// Start position in the texture to apply the delta.
    pub pos: UVec2,
    /// Size of the region to copy.
    pub size: UVec2,
    /// Staging buffer with the image data.
    pub buffer: Buffer,
}

impl GuiRenderer {
    pub fn new(ctx: &Context, layouts: &Layouts) -> Self {
        let vertex = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./gui.vert.spv")),
                debug_name: Some("gui_vertex_shader".into()),
            },
        )
        .unwrap();

        let fragment = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!(concat!(env!("OUT_DIR"), "./gui.frag.spv")),
                debug_name: Some("gui_fragment_shader".into()),
            },
        )
        .unwrap();

        let font_pipeline = GraphicsPipeline::new(
            ctx.clone(),
            GraphicsPipelineCreateInfo {
                stages: ShaderStages::Traditional {
                    vertex,
                    fragment: Some(fragment),
                },
                layouts: vec![layouts.gui.clone()],
                vertex_input: VertexInputState {
                    attributes: vec![
                        // Position
                        VertexInputAttribute {
                            binding: 0,
                            location: 0,
                            format: Format::Rg32SFloat,
                            offset: 0,
                        },
                        // UV
                        VertexInputAttribute {
                            binding: 0,
                            location: 1,
                            format: Format::Rg32SFloat,
                            offset: 2 * std::mem::size_of::<f32>() as u32,
                        },
                        // Color
                        VertexInputAttribute {
                            binding: 0,
                            location: 2,
                            format: Format::Rgba8Unorm,
                            offset: 4 * std::mem::size_of::<f32>() as u32,
                        },
                    ],
                    bindings: vec![VertexInputBinding {
                        binding: 0,
                        stride: std::mem::size_of::<egui::epaint::Vertex>() as u32,
                        input_rate: VertexInputRate::Vertex,
                    }],
                    topology: PrimitiveTopology::TriangleList,
                },
                rasterization: RasterizationState {
                    polygon_mode: PolygonMode::Fill,
                    cull_mode: CullMode::None,
                    front_face: FrontFace::Clockwise,
                },
                depth_stencil: None,
                color_blend: ColorBlendState {
                    attachments: vec![ColorBlendAttachment {
                        write_mask: ColorComponents::R
                            | ColorComponents::G
                            | ColorComponents::B
                            | ColorComponents::A,
                        blend: true,
                        color_blend_op: BlendOp::Add,
                        src_color_blend_factor: BlendFactor::One,
                        dst_color_blend_factor: BlendFactor::OneMinusSrcAlpha,
                        alpha_blend_op: BlendOp::Add,
                        src_alpha_blend_factor: BlendFactor::OneMinusDstAlpha,
                        dst_alpha_blend_factor: BlendFactor::One,
                    }],
                },
                push_constants_size: Some(std::mem::size_of::<GpuGuiPushConstants>() as u32),
                debug_name: Some(String::from("egui_font_pipeline")),
            },
        )
        .unwrap();

        let font_texture = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: Format::Rgba8Unorm,
                ty: TextureType::Type2D,
                width: 1,
                height: 1,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                texture_usage: TextureUsage::TRANSFER_DST | TextureUsage::SAMPLED,
                memory_usage: MemoryUsage::GpuOnly,
                sample_count: MultiSamples::Count1,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some(String::from("egui_font_texture")),
            },
        )
        .unwrap();

        let vertex_buffer = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of::<egui::epaint::Vertex>() as u64 * DEFAULT_VB_SIZE,
                array_elements: FRAMES_IN_FLIGHT,
                buffer_usage: BufferUsage::VERTEX_BUFFER,
                memory_usage: MemoryUsage::CpuToGpu,
                sharing_mode: SharingMode::Exclusive,
                queue_types: QueueTypes::MAIN,
                debug_name: Some(String::from("egui_vertex_buffer")),
            },
        )
        .unwrap();

        let index_buffer = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of::<u32>() as u64 * DEFAULT_IB_SIZE,
                array_elements: FRAMES_IN_FLIGHT,
                buffer_usage: BufferUsage::INDEX_BUFFER,
                memory_usage: MemoryUsage::CpuToGpu,
                sharing_mode: SharingMode::Exclusive,
                queue_types: QueueTypes::MAIN,
                debug_name: Some(String::from("egui_index_buffer")),
            },
        )
        .unwrap();

        let sets = std::array::from_fn(|i| {
            let mut set = DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: layouts.gui.clone(),
                    debug_name: Some(format!("font_set_{i}")),
                },
            )
            .unwrap();

            set.update(&[DescriptorSetUpdate {
                binding: GUI_SET_FONT_BINDING,
                array_element: 0,
                value: DescriptorValue::Texture {
                    texture: &font_texture,
                    array_element: 0,
                    sampler: FONT_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            }]);

            set
        });

        Self {
            ctx: ctx.clone(),
            font_texture,
            vertex_buffer,
            index_buffer,
            font_pipeline,
            sets,
            draw_calls: Vec::default(),
            texture_deltas: Vec::default(),
        }
    }

    pub fn prepare(&mut self, args: GuiDrawPrepare) {
        // Update the lengths of index and vertex buffers if needed
        let mut vb_size_req = 0;
        let mut ib_size_req = 0;
        for primitive in &args.gui_output.primitives {
            match &primitive.primitive {
                egui::epaint::Primitive::Mesh(mesh) => {
                    vb_size_req +=
                        (mesh.vertices.len() * std::mem::size_of::<egui::epaint::Vertex>()) as u64;
                    ib_size_req += (mesh.indices.len() * std::mem::size_of::<u32>()) as u64;
                }
                egui::epaint::Primitive::Callback(_) => warn!("unsupported egui callback"),
            }
        }

        // Resize buffers if needed
        if let Some(new_vb) = Buffer::expand(&self.vertex_buffer, vb_size_req, false) {
            self.vertex_buffer = new_vb;
        }
        if let Some(new_ib) = Buffer::expand(&self.index_buffer, ib_size_req, false) {
            self.index_buffer = new_ib;
        }

        // Prepare draw calls
        let ppp = args.gui_output.pixels_per_point;
        let mut vb_offset = 0;
        let mut ib_offset = 0;

        self.draw_calls.clear();

        let mut vb_view = self.vertex_buffer.write(usize::from(args.frame)).unwrap();
        let vb_slice = bytemuck::cast_slice_mut::<_, egui::epaint::Vertex>(vb_view.as_mut());
        let mut ib_view = self.index_buffer.write(usize::from(args.frame)).unwrap();
        let ib_slice = bytemuck::cast_slice_mut::<_, u32>(ib_view.as_mut());

        for primitive in &args.gui_output.primitives {
            let mesh = match &primitive.primitive {
                egui::epaint::Primitive::Mesh(mesh) => mesh,
                egui::epaint::Primitive::Callback(_) => continue,
            };

            vb_slice[vb_offset..(vb_offset + mesh.vertices.len())].copy_from_slice(&mesh.vertices);
            ib_slice[ib_offset..(ib_offset + mesh.indices.len())].copy_from_slice(&mesh.indices);

            let clip_min_x = ppp * primitive.clip_rect.min.x;
            let clip_min_y = ppp * primitive.clip_rect.min.y;
            let clip_max_x = ppp * primitive.clip_rect.max.x;
            let clip_max_y = ppp * primitive.clip_rect.max.y;

            let clip_min_x = clip_min_x.clamp(0.0, args.canvas_size.0 as f32);
            let clip_min_y = clip_min_y.clamp(0.0, args.canvas_size.1 as f32);
            let clip_max_x = clip_max_x.clamp(clip_min_x, args.canvas_size.0 as f32);
            let clip_max_y = clip_max_y.clamp(clip_min_y, args.canvas_size.1 as f32);

            let clip_min_x = clip_min_x.round() as u32;
            let clip_min_y = clip_min_y.round() as u32;
            let clip_max_x = clip_max_x.round() as u32;
            let clip_max_y = clip_max_y.round() as u32;

            self.draw_calls.push(DrawCall {
                vertex_offset: vb_offset as isize,
                index_offset: ib_offset,
                index_count: mesh.indices.len(),
                scissor: Scissor {
                    x: clip_min_x as i32,
                    y: clip_min_y as i32,
                    width: clip_max_x - clip_min_x,
                    height: clip_max_y - clip_min_y,
                },
                _texture_id: match mesh.texture_id {
                    egui::TextureId::Managed(_) => u32::MAX,
                    egui::TextureId::User(id) => id as u32,
                },
            });

            vb_offset += mesh.vertices.len();
            ib_offset += mesh.indices.len();
        }

        // Handle texture updates
        self.texture_deltas.clear();
        for (id, delta) in &args.gui_output.full.textures_delta.set {
            match id {
                egui::TextureId::Managed(id) => {
                    if *id != 0 {
                        warn!("no support for non-font textures in egui.");
                        continue;
                    }
                }
                egui::TextureId::User(_) => {
                    warn!("no support for user textures in egui");
                    continue;
                }
            }

            let pos = UVec2::from(delta.pos.unwrap_or_default().map(|val| val as u32));
            let size = UVec2::new(delta.image.width() as u32, delta.image.height() as u32);
            let data = match &delta.image {
                egui::ImageData::Color(_) => {
                    warn!("no support for non-font textures in egui.");
                    continue;
                }
                egui::ImageData::Font(img) => img
                    .srgba_pixels(None)
                    .map(|color| color.to_array())
                    .collect::<Vec<_>>(),
            };

            // If the texture size mismatches, we recreate and rebind the texture
            let (width, height, _) = self.font_texture.dims();
            if width < size.x || height < size.y {
                self.font_texture = Texture::new(
                    self.ctx.clone(),
                    TextureCreateInfo {
                        format: Format::Rgba8Unorm,
                        ty: TextureType::Type2D,
                        width: size.x,
                        height: size.y,
                        depth: 1,
                        array_elements: 1,
                        mip_levels: 1,
                        texture_usage: TextureUsage::TRANSFER_DST | TextureUsage::SAMPLED,
                        memory_usage: MemoryUsage::GpuOnly,
                        sharing_mode: SharingMode::Exclusive,
                        queue_types: QueueTypes::MAIN,
                        sample_count: MultiSamples::Count1,
                        debug_name: Some(String::from("egui_font_texture")),
                    },
                )
                .unwrap();

                self.sets.iter_mut().for_each(|set| {
                    set.update(&[DescriptorSetUpdate {
                        binding: 0,
                        array_element: 0,
                        value: DescriptorValue::Texture {
                            texture: &self.font_texture,
                            array_element: 0,
                            sampler: FONT_SAMPLER,
                            base_mip: 0,
                            mip_count: 1,
                        },
                    }]);
                });
            }

            self.texture_deltas.push(TextureDelta {
                pos,
                size,
                buffer: Buffer::new_staging(
                    self.ctx.clone(),
                    QueueType::Main,
                    Some(String::from("texture_del_staging_buffer")),
                    bytemuck::cast_slice(&data),
                )
                .unwrap(),
            })
        }
    }

    pub fn update_textures<'a>(&'a self, commands: &mut CommandBuffer<'a>) {
        self.texture_deltas.iter().for_each(|delta| {
            commands.copy_buffer_to_texture(
                &self.font_texture,
                &delta.buffer,
                BufferTextureCopy {
                    buffer_offset: 0,
                    buffer_row_length: 0,
                    buffer_image_height: 0,
                    buffer_array_element: 0,
                    texture_offset: (delta.pos.x, delta.pos.y, 0),
                    texture_extent: (delta.size.x, delta.size.y, 1),
                    texture_mip_level: 0,
                    texture_array_element: 0,
                },
            );
        });
    }

    pub fn render<'a>(&'a self, frame: Frame, screen_size: (u32, u32), pass: &mut RenderPass<'a>) {
        pass.bind_pipeline(self.font_pipeline.clone());
        pass.bind_sets(0, vec![&self.sets[usize::from(frame)]]);
        pass.bind_vertex_buffers(
            0,
            vec![VertexBind {
                buffer: &self.vertex_buffer,
                array_element: usize::from(frame),
                offset: 0,
            }],
        );
        pass.bind_index_buffer(&self.index_buffer, usize::from(frame), 0, IndexType::U32);

        let constants = [GpuGuiPushConstants {
            screen_size: Vec2::new(screen_size.0 as f32, screen_size.1 as f32),
            texture_id: 0,
        }];
        pass.push_constants(bytemuck::cast_slice(&constants));

        self.draw_calls.iter().for_each(|draw| {
            if draw.scissor.width == 0 || draw.scissor.height == 0 {
                return;
            }

            pass.set_scissor(0, draw.scissor);
            pass.draw_indexed(
                draw.index_count,
                1,
                draw.index_offset,
                draw.vertex_offset,
                0,
            );
        });
    }
}
