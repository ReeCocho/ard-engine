use std::time::Duration;

use ard_engine::{
    core::prelude::*,
    ecs::prelude::*,
    graphics::prelude::*,
    math::{Mat4, Vec2, Vec3, Vec4},
    window::prelude::*,
};

fn main() {
    AppBuilder::new()
        .add_plugin(ArdCorePlugin)
        .add_plugin(WindowPlugin {
            add_primary_window: Some(WindowDescriptor {
                width: 1280.0,
                height: 720.0,
                title: String::from("Textures"),
                ..Default::default()
            }),
            exit_on_close: true,
        })
        .add_plugin(WinitPlugin)
        .add_plugin(VkGraphicsPlugin {
            context_create_info: GraphicsContextCreateInfo {
                window: WindowId::primary(),
                debug: true,
            },
        })
        .add_startup_function(setup)
        .run();
}

struct TextureUpdate {
    texture_to_update: Texture,
    time_to_update: Duration,
    update_count: usize,
}

impl SystemState for TextureUpdate {}

impl Into<System> for TextureUpdate {
    fn into(self) -> System {
        SystemBuilder::new(self)
            .with_handler(TextureUpdate::tick)
            .build()
    }
}

impl TextureUpdate {
    fn tick(&mut self, tick: Tick, _: Commands, _: Queries<()>, res: Res<(Write<Factory>,)>) {
        let res = res.get();
        let factory = res.0.unwrap();

        self.time_to_update += tick.0;
        if self.update_count < 2 && self.time_to_update.as_secs() >= 3 {
            if self.update_count == 0 {
                // Update mip LOD1
                let data = [
                    0u8, 255, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 255, 0, 255,
                ];

                factory.load_texture_mip(&self.texture_to_update, 1, &data);
            } else {
                // Update mip LOD0
                let data = [
                    0u8, 255, 0, 255, 255, 255, 255, 255, 0u8, 255, 0, 255, 255, 255, 255, 255,
                    255, 255, 255, 255, 0, 255, 0, 255, 255, 255, 255, 255, 0, 255, 0, 255, 0u8,
                    255, 0, 255, 255, 255, 255, 255, 0u8, 255, 0, 255, 255, 255, 255, 255, 255,
                    255, 255, 255, 0, 255, 0, 255, 255, 255, 255, 255, 0, 255, 0, 255,
                ];

                factory.load_texture_mip(&self.texture_to_update, 0, &data);
            }

            self.time_to_update = Duration::ZERO;
            self.update_count += 1;
        }
    }
}

fn setup(app: &mut App) {
    let factory = app.resources.get::<Factory>().unwrap();

    // Texture
    let pixel: [u8; 4] = [255, 0, 0, 255];

    let create_info = TextureCreateInfo {
        width: 4,
        height: 4,
        format: TextureFormat::R8G8B8A8Unorm,
        data: &pixel,
        mip_type: MipType::Upload,
        mip_count: 3,
        sampler: SamplerDescriptor {
            min_filter: TextureFilter::Nearest,
            max_filter: TextureFilter::Nearest,
            mip_filter: TextureFilter::Nearest,
            x_tiling: TextureTiling::Repeat,
            y_tiling: TextureTiling::Repeat,
            anisotropic_filtering: false,
        },
    };

    let texture = factory.create_texture(&create_info);

    // Shaders
    let create_info = ShaderCreateInfo {
        ty: ShaderType::Vertex,
        code: include_bytes!("./textured.vert.spv"),
        vertex_layout: VertexLayout {
            uv0: true,
            ..Default::default()
        },
        inputs: ShaderInputs {
            ubo_size: 0,
            texture_count: 1,
        },
    };

    let vert_shader = factory.create_shader(&create_info);

    let create_info = ShaderCreateInfo {
        ty: ShaderType::Fragment,
        code: include_bytes!("./textured.frag.spv"),
        vertex_layout: VertexLayout::default(),
        inputs: ShaderInputs {
            ubo_size: 0,
            texture_count: 1,
        },
    };

    let frag_shader = factory.create_shader(&create_info);

    // Pipeline
    let create_info = PipelineCreateInfo {
        vertex: vert_shader.clone(),
        fragment: frag_shader.clone(),
    };

    let pipeline = factory.create_pipeline(&create_info);

    // Create material
    let create_info = MaterialCreateInfo { pipeline };

    let material = factory.create_material(&create_info);

    // Update material with texture
    factory.update_material_texture(&material, &texture, 0);

    // Quad
    let positions = [
        Vec4::new(-0.5, -0.5, 0.0, 1.0),
        Vec4::new(-0.5, 0.5, 0.0, 1.0),
        Vec4::new(0.5, 0.5, 0.0, 1.0),
        Vec4::new(-0.5, -0.5, 0.0, 1.0),
        Vec4::new(0.5, 0.5, 0.0, 1.0),
        Vec4::new(0.5, -0.5, 0.0, 1.0),
    ];

    let uvs = [
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(1.0, 0.0),
    ];

    let indices = [0, 1, 2, 3, 4, 5];

    let create_info = MeshCreateInfo {
        positions: &positions,
        uv0: Some(&uvs),
        indices: &indices,
        bounds: MeshBounds::Generate,
        ..Default::default()
    };

    let quad_mesh = factory.create_mesh(&create_info);

    // Create textured quad
    let draws = app.resources.get_mut::<StaticGeometry>().unwrap();

    draws.register(
        &[(
            Renderable {
                mesh: quad_mesh,
                material,
            },
            Model(Mat4::from_translation(Vec3::new(0.0, 0.0, 1.0))),
        )],
        &mut [],
    );

    app.dispatcher.add_system(TextureUpdate {
        texture_to_update: texture,
        time_to_update: Duration::ZERO,
        update_count: 0,
    });
}
