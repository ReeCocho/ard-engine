use ard_ecs::{
    prelude::{Commands, Everything, Queries, Res},
    resource::Resource,
};
use ard_input::{InputState, Key, MouseButton};
use ard_log::warn;
use ard_math::{IVec2, UVec2, Vec2};
use ard_pal::prelude::*;
use ard_window::window::Window;
use bytemuck::{Pod, Zeroable};
use ordered_float::NotNan;

use crate::{factory::textures::TextureSets, shader_constants::FRAMES_IN_FLIGHT};

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

#[derive(Resource)]
pub struct Gui {
    ctx: Context,
    views: Vec<Box<dyn View + 'static>>,
    egui: egui::Context,
    input: egui::RawInput,
    _layout: DescriptorSetLayout,
    sets: Vec<DescriptorSet>,
    font_pipeline: GraphicsPipeline,
    tex_pipeline: GraphicsPipeline,
    font_texture: Texture,
    vertex_buffer: Buffer,
    index_buffer: Buffer,
    draw_calls: Vec<DrawCall>,
    texture_deltas: Vec<TextureDelta>,
}

pub trait View {
    fn show(
        &mut self,
        ctx: &egui::Context,
        commands: &Commands,
        queries: &Queries<Everything>,
        res: &Res<Everything>,
    );
}

pub(crate) struct GuiPrepareDraw<'a> {
    pub frame: usize,
    pub scene_tex: (&'a Texture, usize),
    pub canvas_size: IVec2,
    pub dt: f32,
    pub commands: &'a ard_ecs::prelude::Commands,
    pub queries: &'a Queries<Everything>,
    pub res: &'a Res<Everything>,
}

#[allow(dead_code)]
#[derive(Copy, Clone)]
struct GuiPushConstants {
    screen_size: Vec2,
    texture_id: u32,
}

struct DrawCall {
    vertex_offset: isize,
    index_offset: usize,
    index_count: usize,
    scissor: Scissor,
    texture_id: u32,
}

struct TextureDelta {
    /// Start position in the texture to apply the delta.
    pub pos: UVec2,
    /// Size of the region to copy.
    pub size: UVec2,
    /// Staging buffer with the image data.
    pub buffer: Buffer,
}

unsafe impl Pod for GuiPushConstants {}
unsafe impl Zeroable for GuiPushConstants {}

// API
impl Gui {
    pub fn add_view(&mut self, view: impl View + 'static) {
        self.views.push(Box::new(view));
    }
}

// Internal
impl Gui {
    pub(crate) fn new(ctx: Context, textures_layout: &DescriptorSetLayout) -> Self {
        let layout = DescriptorSetLayout::new(
            ctx.clone(),
            DescriptorSetLayoutCreateInfo {
                bindings: vec![
                    // Font texture
                    DescriptorBinding {
                        binding: 0,
                        ty: DescriptorType::Texture,
                        count: 1,
                        stage: ShaderStage::Fragment,
                    },
                    // Scene texture
                    DescriptorBinding {
                        binding: 1,
                        ty: DescriptorType::Texture,
                        count: 1,
                        stage: ShaderStage::Fragment,
                    },
                ],
            },
        )
        .unwrap();

        let vertex = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!("../shaders/gui.vert.spv"),
                debug_name: Some(String::from("egui_vertex_shader")),
            },
        )
        .unwrap();

        let fragment = Shader::new(
            ctx.clone(),
            ShaderCreateInfo {
                code: include_bytes!("../shaders/gui.frag.spv"),
                debug_name: Some(String::from("egui_fragment_shader")),
            },
        )
        .unwrap();

        let font_pipeline = GraphicsPipeline::new(
            ctx.clone(),
            GraphicsPipelineCreateInfo {
                stages: ShaderStages {
                    vertex: vertex.clone(),
                    fragment: Some(fragment.clone()),
                },
                layouts: vec![layout.clone(), textures_layout.clone()],
                vertex_input: VertexInputState {
                    attributes: vec![
                        // Position
                        VertexInputAttribute {
                            binding: 0,
                            location: 0,
                            format: VertexFormat::XyF32,
                            offset: 0,
                        },
                        // UV
                        VertexInputAttribute {
                            binding: 0,
                            location: 1,
                            format: VertexFormat::XyF32,
                            offset: 2 * std::mem::size_of::<f32>() as u32,
                        },
                        // Color
                        VertexInputAttribute {
                            binding: 0,
                            location: 2,
                            format: VertexFormat::XyzwU8,
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
                color_blend: Some(ColorBlendState {
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
                }),
                push_constants_size: Some(std::mem::size_of::<GuiPushConstants>() as u32),
                debug_name: Some(String::from("egui_font_pipeline")),
            },
        )
        .unwrap();

        let tex_pipeline = GraphicsPipeline::new(
            ctx.clone(),
            GraphicsPipelineCreateInfo {
                stages: ShaderStages {
                    vertex,
                    fragment: Some(fragment),
                },
                layouts: vec![layout.clone(), textures_layout.clone()],
                vertex_input: VertexInputState {
                    attributes: vec![
                        // Position
                        VertexInputAttribute {
                            binding: 0,
                            location: 0,
                            format: VertexFormat::XyF32,
                            offset: 0,
                        },
                        // UV
                        VertexInputAttribute {
                            binding: 0,
                            location: 1,
                            format: VertexFormat::XyF32,
                            offset: 2 * std::mem::size_of::<f32>() as u32,
                        },
                        // Color
                        VertexInputAttribute {
                            binding: 0,
                            location: 2,
                            format: VertexFormat::XyzwU8,
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
                color_blend: Some(ColorBlendState {
                    attachments: vec![ColorBlendAttachment {
                        write_mask: ColorComponents::R
                            | ColorComponents::G
                            | ColorComponents::B
                            | ColorComponents::A,
                        blend: true,
                        color_blend_op: BlendOp::Add,
                        src_color_blend_factor: BlendFactor::SrcAlpha,
                        dst_color_blend_factor: BlendFactor::OneMinusSrcAlpha,
                        alpha_blend_op: BlendOp::Add,
                        src_alpha_blend_factor: BlendFactor::One,
                        dst_alpha_blend_factor: BlendFactor::Zero,
                    }],
                }),
                push_constants_size: Some(std::mem::size_of::<GuiPushConstants>() as u32),
                debug_name: Some(String::from("egui_tex_pipeline")),
            },
        )
        .unwrap();

        let font_texture = Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: TextureFormat::Rgba8Unorm,
                ty: TextureType::Type2D,
                width: 1,
                height: 1,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                texture_usage: TextureUsage::TRANSFER_DST | TextureUsage::SAMPLED,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("egui_font_texture")),
            },
        )
        .unwrap();

        let mut sets = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for frame in 0..FRAMES_IN_FLIGHT {
            let mut set = DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: layout.clone(),
                    debug_name: Some(format!("egui_font_set_{frame}")),
                },
            )
            .unwrap();

            set.update(&[DescriptorSetUpdate {
                binding: 0,
                array_element: 0,
                value: DescriptorValue::Texture {
                    texture: &font_texture,
                    array_element: 0,
                    sampler: FONT_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            }]);

            sets.push(set);
        }

        let vertex_buffer = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of::<egui::epaint::Vertex>() as u64 * DEFAULT_VB_SIZE,
                array_elements: FRAMES_IN_FLIGHT,
                buffer_usage: BufferUsage::VERTEX_BUFFER,
                memory_usage: MemoryUsage::CpuToGpu,
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
                debug_name: Some(String::from("egui_index_buffer")),
            },
        )
        .unwrap();

        Self {
            ctx,
            views: Vec::default(),
            egui: egui::Context::default(),
            input: egui::RawInput::default(),
            _layout: layout,
            sets,
            font_pipeline,
            tex_pipeline,
            font_texture,
            vertex_buffer,
            index_buffer,
            draw_calls: Vec::default(),
            texture_deltas: Vec::default(),
        }
    }

    pub(crate) fn update_input(&mut self, input: &InputState, window: &Window) {
        // Don't bother gathering input if the mouse is locked
        if window.cursor_locked() {
            return;
        }

        // Copy-paste and text input
        if !input.input_string().is_empty() {
            let mut final_txt = String::with_capacity(input.input_string().bytes().len());

            // Ignore non-printable characters
            for chr in input.input_string().chars() {
                // Gonna be real, I grabbed this from the egui-winit integration. Idk how it works
                let is_printable_char = {
                    let is_in_private_use_area = ('\u{e000}'..='\u{f8ff}').contains(&chr)
                        || ('\u{f0000}'..='\u{ffffd}').contains(&chr)
                        || ('\u{100000}'..='\u{10fffd}').contains(&chr);

                    !is_in_private_use_area && !chr.is_ascii_control()
                };

                if is_printable_char {
                    final_txt.push(chr);
                }
            }

            self.input.events.push(egui::Event::Text(final_txt));
        }

        // Modifiers
        self.input.modifiers.alt = input.key(Key::LAlt) || input.key(Key::RAlt);
        self.input.modifiers.ctrl = input.key(Key::LCtrl) || input.key(Key::RCtrl);
        self.input.modifiers.shift = input.key(Key::LShift) || input.key(Key::RShift);

        // Mouse buttons
        self.handle_mouse_button(MouseButton::Left, egui::PointerButton::Primary, input);
        self.handle_mouse_button(MouseButton::Right, egui::PointerButton::Secondary, input);
        self.handle_mouse_button(MouseButton::Middle, egui::PointerButton::Middle, input);

        // Mouse movement
        let (del_x, del_y) = input.mouse_delta();
        if del_x != 0.0 || del_y != 0.0 {
            let (x, y) = input.mouse_pos();
            self.input
                .events
                .push(egui::Event::PointerMoved(egui::Pos2::new(
                    x as f32, y as f32,
                )));
        }

        // Keyboard input
        fn keyboard_input(
            ard_key: Key,
            egui_key: egui::Key,
            ard_input: &InputState,
            egui_input: &mut egui::RawInput,
        ) {
            if ard_input.key_down_repeat(ard_key) {
                egui_input.events.push(egui::Event::Key {
                    key: egui_key,
                    pressed: true,
                    modifiers: egui_input.modifiers,
                });
            }
        }

        keyboard_input(Key::Left, egui::Key::ArrowLeft, input, &mut self.input);
        keyboard_input(Key::Right, egui::Key::ArrowRight, input, &mut self.input);
        keyboard_input(Key::Up, egui::Key::ArrowUp, input, &mut self.input);
        keyboard_input(Key::Down, egui::Key::ArrowDown, input, &mut self.input);

        keyboard_input(Key::Escape, egui::Key::Escape, input, &mut self.input);
        keyboard_input(Key::Tab, egui::Key::Tab, input, &mut self.input);
        keyboard_input(Key::Back, egui::Key::Backspace, input, &mut self.input);
        keyboard_input(Key::Return, egui::Key::Enter, input, &mut self.input);
        keyboard_input(Key::Space, egui::Key::Space, input, &mut self.input);

        keyboard_input(Key::Insert, egui::Key::Insert, input, &mut self.input);
        keyboard_input(Key::Delete, egui::Key::Delete, input, &mut self.input);
        keyboard_input(Key::Home, egui::Key::Home, input, &mut self.input);
        keyboard_input(Key::End, egui::Key::End, input, &mut self.input);
        keyboard_input(Key::PageUp, egui::Key::PageUp, input, &mut self.input);
        keyboard_input(Key::PageDown, egui::Key::PageDown, input, &mut self.input);
    }

    fn handle_mouse_button(
        &mut self,
        button: MouseButton,
        egui_button: egui::PointerButton,
        input: &InputState,
    ) {
        if input.mouse_button_down(button) {
            let (x, y) = input.mouse_pos();
            self.input.events.push(egui::Event::PointerButton {
                pos: egui::Pos2::new(x as f32, y as f32),
                button: egui_button,
                pressed: true,
                modifiers: self.input.modifiers,
            })
        }

        if input.mouse_button_up(button) {
            let (x, y) = input.mouse_pos();
            self.input.events.push(egui::Event::PointerButton {
                pos: egui::Pos2::new(x as f32, y as f32),
                button: egui_button,
                pressed: false,
                modifiers: self.input.modifiers,
            })
        }
    }

    pub(crate) fn prepare_draw(&mut self, args: GuiPrepareDraw) {
        self.input.predicted_dt = args.dt;
        self.input.screen_rect = Some(egui::Rect {
            min: egui::Pos2::ZERO,
            max: egui::Pos2::new(args.canvas_size.x as f32, args.canvas_size.y as f32),
        });

        // Bind scene texture
        self.sets[args.frame].update(&[DescriptorSetUpdate {
            binding: 1,
            array_element: 0,
            value: DescriptorValue::Texture {
                texture: args.scene_tex.0,
                array_element: args.scene_tex.1,
                sampler: FONT_SAMPLER,
                base_mip: 0,
                mip_count: 1,
            },
        }]);

        // Draw all views
        let raw_input = std::mem::take(&mut self.input);
        let full_output = self.egui.run(raw_input, |ctx| {
            for view in &mut self.views {
                view.show(ctx, args.commands, args.queries, args.res)
            }
        });

        // Tesselate output
        let primitives = self.egui.tessellate(full_output.shapes);

        // Update the lengths of index and vertex buffers if needed
        let mut vb_size_req = 0;
        let mut ib_size_req = 0;
        for primitive in &primitives {
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
        let ppp = self.egui.pixels_per_point();
        let mut vb_offset = 0;
        let mut ib_offset = 0;

        self.draw_calls.clear();

        let mut vb_view = self.vertex_buffer.write(args.frame).unwrap();
        let vb_slice = bytemuck::cast_slice_mut::<_, egui::epaint::Vertex>(vb_view.as_mut());
        let mut ib_view = self.index_buffer.write(args.frame).unwrap();
        let ib_slice = bytemuck::cast_slice_mut::<_, u32>(ib_view.as_mut());

        for primitive in &primitives {
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

            let clip_min_x = clip_min_x.clamp(0.0, args.canvas_size.x as f32);
            let clip_min_y = clip_min_y.clamp(0.0, args.canvas_size.y as f32);
            let clip_max_x = clip_max_x.clamp(clip_min_x, args.canvas_size.x as f32);
            let clip_max_y = clip_max_y.clamp(clip_min_y, args.canvas_size.y as f32);

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
                texture_id: match mesh.texture_id {
                    egui::TextureId::Managed(_) => u32::MAX,
                    egui::TextureId::User(id) => id as u32,
                },
            });

            vb_offset += mesh.vertices.len();
            ib_offset += mesh.indices.len();
        }

        // Handle texture updates
        self.texture_deltas.clear();
        for (id, delta) in &full_output.textures_delta.set {
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
                        format: TextureFormat::Rgba8Unorm,
                        ty: TextureType::Type2D,
                        width: size.x,
                        height: size.y,
                        depth: 1,
                        array_elements: 1,
                        mip_levels: 1,
                        texture_usage: TextureUsage::TRANSFER_DST | TextureUsage::SAMPLED,
                        memory_usage: MemoryUsage::GpuOnly,
                        debug_name: Some(String::from("egui_font_texture")),
                    },
                )
                .unwrap();

                for frame in 0..FRAMES_IN_FLIGHT {
                    self.sets[frame].update(&[DescriptorSetUpdate {
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
                }
            }

            self.texture_deltas.push(TextureDelta {
                pos,
                size,
                buffer: Buffer::new_staging(
                    self.ctx.clone(),
                    Some(String::from("texture_del_staging_buffer")),
                    bytemuck::cast_slice(&data),
                )
                .unwrap(),
            })
        }
    }

    pub(crate) fn update_textures<'a, 'b>(&'a self, commands: &'b mut CommandBuffer<'a>) {
        for delta in &self.texture_deltas {
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
        }
    }

    pub(crate) fn draw<'a, 'b>(
        &'a self,
        frame: usize,
        screen_size: Vec2,
        textures_sets: &'a TextureSets,
        pass: &'b mut RenderPass<'a>,
    ) {
        pass.bind_pipeline(self.font_pipeline.clone());
        pass.bind_sets(0, vec![&self.sets[frame], textures_sets.set(frame)]);
        pass.bind_vertex_buffers(
            0,
            vec![VertexBind {
                buffer: &self.vertex_buffer,
                array_element: frame,
                offset: 0,
            }],
        );
        pass.bind_index_buffer(&self.index_buffer, frame, 0, IndexType::U32);

        let mut last_texture_id = u32::MAX;
        let constants = [GuiPushConstants {
            screen_size,
            texture_id: last_texture_id,
        }];
        pass.push_constants(bytemuck::cast_slice(&constants));

        for draw in &self.draw_calls {
            if draw.texture_id != last_texture_id {
                if draw.texture_id == u32::MAX {
                    pass.bind_pipeline(self.font_pipeline.clone());
                } else {
                    pass.bind_pipeline(self.tex_pipeline.clone());
                }

                let constants = [GuiPushConstants {
                    screen_size,
                    texture_id: draw.texture_id,
                }];
                pass.push_constants(bytemuck::cast_slice(&constants));
                last_texture_id = draw.texture_id;
            }

            if draw.scissor.width == 0 || draw.scissor.height == 0 {
                continue;
            }

            pass.set_scissor(0, draw.scissor);
            pass.draw_indexed(
                draw.index_count,
                1,
                draw.index_offset,
                draw.vertex_offset,
                0,
            );
        }
    }
}
