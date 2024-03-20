use std::time::Instant;

use ard_assets::prelude::{AssetName, Assets, AssetsPlugin};
use ard_core::prelude::*;
use ard_ecs::prelude::*;
use ard_input::{InputState, Key};
use ard_math::*;
use ard_pal::prelude::*;
use ard_render::{
    factory::Factory, system::PostRender, MsaaSettings, RenderPlugin, RendererSettings,
};
use ard_render_assets::{model::ModelAsset, RenderAssetsPlugin};
use ard_render_camera::{Camera, CameraClearColor};
use ard_render_gui::{view::GuiView, Gui};
use ard_render_image_effects::{
    ao::AoSettings, smaa::SmaaSettings, sun_shafts2::SunShaftsSettings,
    tonemapping::TonemappingSettings,
};
use ard_render_lighting::{global::GlobalLighting, Light};
use ard_render_meshes::{mesh::MeshCreateInfo, vertices::VertexAttributes};
use ard_render_objects::{Model, RenderFlags, RenderingMode};
use ard_render_pbr::PbrMaterialData;
use ard_window::prelude::*;
use ard_winit::prelude::*;

#[derive(SystemState)]
pub struct FrameRate {
    frame_ctr: usize,
    last_sec: Instant,
}

impl Default for FrameRate {
    fn default() -> Self {
        FrameRate {
            frame_ctr: 0,
            last_sec: Instant::now(),
        }
    }
}

impl FrameRate {
    fn post_render(&mut self, _: PostRender, _: Commands, _: Queries<()>, _: Res<()>) {
        let now = Instant::now();
        self.frame_ctr += 1;
        if now.duration_since(self.last_sec).as_secs_f32() >= 1.0 {
            println!("Frame Rate: {}", self.frame_ctr);
            self.last_sec = now;
            self.frame_ctr = 0;
        }
    }
}

impl Into<System> for FrameRate {
    fn into(self) -> System {
        SystemBuilder::new(self)
            .with_handler(FrameRate::post_render)
            .build()
    }
}

#[derive(SystemState)]
pub struct CameraMover {
    pub cursor_locked: bool,
    pub look_speed: f32,
    pub move_speed: f32,
    pub entity: Entity,
    pub position: Vec3,
    pub rotation: Vec3,
}

impl CameraMover {
    fn on_tick(
        &mut self,
        evt: Tick,
        _: Commands,
        queries: Queries<(Write<Camera>, Write<Model>)>,
        res: Res<(Read<Factory>, Read<InputState>, Write<Windows>)>,
    ) {
        let input = res.get::<InputState>().unwrap();
        let mut windows = res.get_mut::<Windows>().unwrap();

        // Rotate the camera
        let delta = evt.0.as_secs_f32();
        if self.cursor_locked {
            let (mx, my) = input.mouse_delta();
            self.rotation.x += (my as f32) * self.look_speed;
            self.rotation.y += (mx as f32) * self.look_speed;
            self.rotation.x = self.rotation.x.clamp(-85.0, 85.0);
        }

        // Direction from rotation
        let rot = Mat4::from_euler(
            EulerRot::YXZ,
            self.rotation.y.to_radians(),
            self.rotation.x.to_radians(),
            0.0,
        );

        // Move the camera
        let right = rot.col(0);
        let forward = rot.col(2);

        if self.cursor_locked {
            if input.key(Key::W) {
                self.position += forward.xyz() * delta * self.move_speed;
            }

            if input.key(Key::S) {
                self.position -= forward.xyz() * delta * self.move_speed;
            }

            if input.key(Key::A) {
                self.position -= right.xyz() * delta * self.move_speed;
            }

            if input.key(Key::D) {
                self.position += right.xyz() * delta * self.move_speed;
            }
        }

        // Lock cursor
        if input.key_up(Key::M) {
            self.cursor_locked = !self.cursor_locked;

            let window = windows.get_mut(WindowId::primary()).unwrap();

            window.set_cursor_lock_mode(self.cursor_locked);
            window.set_cursor_visibility(!self.cursor_locked);
        }

        // Update the camera
        let mut query = queries.get::<Write<Model>>(self.entity).unwrap();
        query.0 = Mat4::from_translation(self.position) * rot;
    }
}

impl From<CameraMover> for System {
    fn from(mover: CameraMover) -> Self {
        SystemBuilder::new(mover)
            .with_handler(CameraMover::on_tick)
            .build()
    }
}

struct TestingGui {
    sun_yaw: f32,
    sun_pitch: f32,
    sun_animation_speed: f32,
    animate_sun: bool,
    welcome_open: bool,
    ui_visible: bool,
}

impl Default for TestingGui {
    fn default() -> Self {
        Self {
            sun_yaw: 45.0,
            sun_pitch: 60.0,
            sun_animation_speed: 3.0,
            animate_sun: false,
            welcome_open: true,
            ui_visible: true,
        }
    }
}

impl GuiView for TestingGui {
    fn show(
        &mut self,
        tick: Tick,
        ctx: &egui::Context,
        _commands: &Commands,
        _queries: &Queries<Everything>,
        res: &Res<Everything>,
    ) {
        let mut lighting = res.get_mut::<GlobalLighting>().unwrap();
        let mut sun_color = lighting.sun_color().to_array();
        let mut sun_intensity = lighting.sun_intensity();
        let mut ambient_color = lighting.ambient_color().to_array();
        let mut ambient_intensity = lighting.ambient_intensity();

        let mut tonemapping = res.get_mut::<TonemappingSettings>().unwrap();
        let mut ao = res.get_mut::<AoSettings>().unwrap();
        let mut sun_shafts = res.get_mut::<SunShaftsSettings>().unwrap();
        let mut smaa = res.get_mut::<SmaaSettings>().unwrap();
        let mut msaa = res.get_mut::<MsaaSettings>().unwrap();

        if self.ui_visible {
            egui::Window::new("Welcome").open(&mut self.welcome_open).show(ctx, |ui| {
                ui.label(
                    "Hello! Welcome to my renderer. This is a general showcase of some of its \
                    features. You can view and modify some settings in the other window that should be \
                    visible. Feel free to play around! If anything looks super broken, please check the \
                    README.txt file to see if it’s a known issue. If it’s not a known issue, shoot me an \
                    email at connor.bramham01@gmail.com."
                );

                ui.separator();
                ui.heading("Controls");

                ui.label("Press M to toggle mouse lock on and off.");
                ui.label("Use the mouse to look around (only works when the mouse is locked).");
                ui.label("W, A, S, D to move.");
            });

            egui::Window::new("Scene Settings")
                .min_width(300.0)
                .max_width(300.0)
                .show(ctx, |ui| {
                    ui.set_width(ui.available_width());

                    egui::CollapsingHeader::new("Sun Lighting").show_unindented(ui, |ui| {
                        egui::Grid::new("_sun_settings_grid").show(ui, |ui| {
                            ui.label("Animate Sun");
                            ui.toggle_value(&mut self.animate_sun, "Toggle");
                            ui.end_row();

                            ui.label("Yaw");
                            ui.add(egui::Slider::new(&mut self.sun_yaw, 0.0..=360.0));
                            ui.end_row();

                            ui.label("Pitch");
                            ui.add_enabled(
                                !self.animate_sun,
                                egui::Slider::new(&mut self.sun_pitch, 0.0..=360.0),
                            );
                            ui.end_row();

                            ui.label("Animation Speed");
                            ui.add(egui::Slider::new(&mut self.sun_animation_speed, 0.0..=32.0));
                            ui.end_row();

                            ui.label("Color");
                            egui::color_picker::color_edit_button_rgb(ui, &mut sun_color);
                            ui.end_row();

                            ui.label("Intensity");
                            ui.add(egui::Slider::new(&mut sun_intensity, 0.0..=64.0));
                            ui.end_row();
                        });
                    });

                    egui::CollapsingHeader::new("Ambient Lighting").show_unindented(ui, |ui| {
                        egui::Grid::new("_ambient_settings_grid").show(ui, |ui| {
                            ui.label("Color");
                            egui::color_picker::color_edit_button_rgb(ui, &mut ambient_color);
                            ui.end_row();

                            ui.label("Intensity");
                            ui.add(egui::Slider::new(&mut ambient_intensity, 0.0..=8.0));
                            ui.end_row();
                        });
                    });

                    egui::CollapsingHeader::new("Shadows").show(ui, |ui| {
                        let mut cascades: Vec<_> = lighting.shadow_cascades().into();

                        for (i, cascade) in cascades.iter_mut().enumerate() {
                            egui::CollapsingHeader::new(format!("Cascade {i}")).show_unindented(
                                ui,
                                |ui| {
                                    egui::Grid::new(format!("_shadow_cascade_{i}_settings_grid"))
                                        .show(ui, |ui| {
                                            ui.label("Min Depth Bias");
                                            ui.add(egui::Slider::new(
                                                &mut cascade.min_depth_bias,
                                                0.0..=8.0,
                                            ));
                                            ui.end_row();

                                            ui.label("Max Depth Bias");
                                            ui.add(egui::Slider::new(
                                                &mut cascade.max_depth_bias,
                                                0.0..=8.0,
                                            ));
                                            ui.end_row();

                                            ui.label("Normal Bias");
                                            ui.add(egui::Slider::new(
                                                &mut cascade.normal_bias,
                                                0.0..=2.0,
                                            ));
                                            ui.end_row();

                                            ui.label("Filter Size");
                                            ui.add(egui::Slider::new(
                                                &mut cascade.filter_size,
                                                0.0..=8.0,
                                            ));
                                            ui.end_row();

                                            ui.label("End Distance");
                                            ui.add(egui::DragValue::new(&mut cascade.end_distance));
                                            ui.end_row();

                                            ui.label("Resolution");
                                            ui.add(egui::DragValue::new(&mut cascade.resolution));
                                            ui.end_row();

                                            cascade.end_distance = cascade.end_distance.max(0.0);
                                            cascade.resolution =
                                                cascade.resolution.clamp(1024, 8192);
                                        });
                                },
                            );
                        }

                        lighting.set_shadow_cascade_settings(&cascades);
                    });

                    egui::CollapsingHeader::new("Tonemapping").show_unindented(ui, |ui| {
                        egui::Grid::new("_tonemapping_settings_grid").show(ui, |ui| {
                            ui.label("Min Luminance");
                            ui.add(egui::Slider::new(
                                &mut tonemapping.min_luminance,
                                -8.0..=8.0,
                            ));
                            ui.end_row();

                            ui.label("Max Luminance");
                            ui.add(egui::Slider::new(
                                &mut tonemapping.max_luminance,
                                -8.0..=8.0,
                            ));
                            ui.end_row();

                            ui.label("Gamma");
                            ui.add(egui::Slider::new(&mut tonemapping.gamma, 0.0..=8.0));
                            ui.end_row();

                            ui.label("Exposure");
                            ui.add(egui::Slider::new(&mut tonemapping.exposure, 0.0..=8.0));
                            ui.end_row();

                            ui.label("Auto-Exposure Rate");
                            ui.add(egui::Slider::new(
                                &mut tonemapping.auto_exposure_rate,
                                0.01..=8.0,
                            ));
                            ui.end_row();
                        });

                        tonemapping.min_luminance =
                            tonemapping.min_luminance.min(tonemapping.max_luminance);
                    });

                    egui::CollapsingHeader::new("Ambient Occlusion").show_unindented(ui, |ui| {
                        egui::Grid::new("_ao_settings_grid").show(ui, |ui| {
                            ui.label("Radius");
                            ui.add(egui::Slider::new(&mut ao.radius, 0.0..=8.0));
                            ui.end_row();

                            ui.label("Falloff Range");
                            ui.add(egui::Slider::new(&mut ao.effect_falloff_range, 0.0..=8.0));
                            ui.end_row();

                            ui.label("Final Value Power");
                            ui.add(egui::Slider::new(&mut ao.final_value_power, 0.0..=8.0));
                            ui.end_row();

                            ui.label("Denoise Blur Beta");
                            ui.add(egui::Slider::new(&mut ao.denoise_blur_beta, 0.0..=4.0));
                            ui.end_row();

                            ui.label("Bilateral Filter D");
                            ui.add(egui::Slider::new(&mut ao.bilateral_filter_d, 0.0..=16.0));
                            ui.end_row();

                            ui.label("Bilateral Filter R");
                            ui.add(egui::Slider::new(&mut ao.bilateral_filter_r, 0.0..=4.0));
                            ui.end_row();

                            ui.label("Sample Distribution Power");
                            ui.add(egui::Slider::new(
                                &mut ao.sample_distribution_power,
                                0.0..=8.0,
                            ));
                            ui.end_row();

                            ui.label("Thin Occluder Compensation");
                            ui.add(egui::Slider::new(
                                &mut ao.thin_occluder_compensation,
                                0.0..=8.0,
                            ));
                            ui.end_row();

                            ui.label("Depth Mip Sampling Offset");
                            ui.add(egui::Slider::new(
                                &mut ao.depth_mip_sampling_offset,
                                0.0..=5.0,
                            ));
                            ui.end_row();
                        });
                    });

                    egui::CollapsingHeader::new("Sun Shafts").show_unindented(ui, |ui| {
                        egui::Grid::new("_sun_shafts_settings_grid").show(ui, |ui| {
                            ui.label("Low Sample Minimum");
                            ui.add(egui::Slider::new(
                                &mut sun_shafts.low_sample_minimum,
                                0..=50,
                            ));
                            ui.end_row();

                            ui.label("Steps Per Sample");
                            ui.add(egui::Slider::new(&mut sun_shafts.steps_per_sample, 0..=200));
                            ui.end_row();

                            ui.label("Depth Threshold");
                            ui.add(egui::Slider::new(
                                &mut sun_shafts.depth_threshold,
                                0.0..=8.0,
                            ));
                            ui.end_row();
                        });
                    });

                    egui::CollapsingHeader::new("Anti-Aliasing Settings").show_unindented(
                        ui,
                        |ui| {
                            egui::Grid::new("_aa_settings_grid").show(ui, |ui| {
                                ui.label("SMAA Enabled");
                                ui.add(egui::Checkbox::new(&mut smaa.enabled, ""));
                                ui.end_row();

                                ui.label("MSAA Setting");
                                egui::ComboBox::new("_msaa_setting", "")
                                    .selected_text(format!("{:?}", msaa.samples))
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut msaa.samples,
                                            MultiSamples::Count1,
                                            "Disabled",
                                        );
                                        ui.selectable_value(
                                            &mut msaa.samples,
                                            MultiSamples::Count2,
                                            "2x",
                                        );
                                        ui.selectable_value(
                                            &mut msaa.samples,
                                            MultiSamples::Count4,
                                            "4x",
                                        );
                                        ui.selectable_value(
                                            &mut msaa.samples,
                                            MultiSamples::Count8,
                                            "8x",
                                        );
                                    });
                            });
                        },
                    );
                });
        }

        if self.animate_sun {
            self.sun_pitch += self.sun_animation_speed * tick.0.as_secs_f32();
            self.sun_pitch -= self.sun_pitch.div_euclid(360.0) * 360.0;
        }

        let sun_dir = (Mat4::from_euler(
            EulerRot::default(),
            self.sun_yaw.to_radians(),
            self.sun_pitch.to_radians(),
            0.0,
        ) * Vec4::Z)
            .xyz();

        lighting.set_sun_direction(sun_dir);
        lighting.set_sun_color(sun_color.into());
        lighting.set_sun_intensity(sun_intensity);
        lighting.set_ambient_color(ambient_color.into());
        lighting.set_ambient_intensity(ambient_intensity);
    }
}

fn main() {
    // let server_addr = format!("127.0.0.1:{}", puffin_http::DEFAULT_PORT);
    // let _puffin_server = puffin_http::Server::new(&server_addr).unwrap();
    // puffin::set_scopes_on(true);

    AppBuilder::new(ard_log::LevelFilter::Info)
        .add_plugin(ArdCorePlugin)
        .add_plugin(WindowPlugin {
            add_primary_window: Some(WindowDescriptor {
                title: String::from("Renderer Testing"),
                resizable: true,
                width: 1280.0,
                height: 720.0,
                ..Default::default()
            }),
            exit_on_close: true,
        })
        .add_plugin(WinitPlugin)
        .add_plugin(AssetsPlugin)
        .add_plugin(RenderPlugin {
            window: WindowId::primary(),
            settings: RendererSettings {
                render_scene: true,
                render_time: None,
                present_mode: PresentMode::Mailbox,
                render_scale: 1.0,
                canvas_size: None,
            },
            debug: true,
        })
        .add_plugin(RenderAssetsPlugin)
        .add_system(FrameRate::default())
        .add_startup_function(setup)
        .run();
}

fn setup(app: &mut App) {
    let factory = app.resources.get::<Factory>().unwrap();
    let assets = app.resources.get::<Assets>().unwrap();
    let mut gui = app.resources.get_mut::<Gui>().unwrap();

    gui.add_view(TestingGui::default());

    // Load in models
    let bistro_model = assets.load::<ModelAsset>(AssetName::new("test_scene.model"));
    let sphere_model = assets.load::<ModelAsset>(AssetName::new("sphere.model"));
    assets.wait_for_load(&bistro_model);
    assets.wait_for_load(&sphere_model);

    // Instantiate models
    let instance = assets.get(&bistro_model).unwrap().instantiate();
    app.world.entities().commands().create(
        (
            vec![Static(0); instance.meshes.meshes.len()],
            instance.meshes.meshes,
            instance.meshes.materials,
            instance.meshes.models,
            instance
                .meshes
                .rendering_mode
                .iter()
                .map(|mode| match *mode {
                    RenderingMode::Opaque => RenderingMode::Opaque,
                    RenderingMode::AlphaCutout => RenderingMode::AlphaCutout,
                    RenderingMode::Transparent => RenderingMode::AlphaCutout,
                })
                .collect(),
            instance.meshes.flags,
        ),
        &mut [],
    );

    let sphere = assets.get(&sphere_model).unwrap();
    let offset = Vec3::new(0.0, 25.0, 0.0);
    const SPHERE_X: usize = 8;
    const SPHERE_Y: usize = 4;
    const SPHERE_COUNT: usize = SPHERE_X * SPHERE_Y;

    let mut pack = (
        vec![Static(0); SPHERE_COUNT],
        (0..SPHERE_COUNT)
            .map(|_| sphere.meshes[0].clone())
            .collect(),
        Vec::with_capacity(SPHERE_COUNT),
        Vec::with_capacity(SPHERE_COUNT),
        (0..SPHERE_COUNT).map(|_| RenderingMode::Opaque).collect(),
        (0..SPHERE_COUNT).map(|_| RenderFlags::empty()).collect(),
    );

    for x in 0..SPHERE_X {
        for y in 0..SPHERE_Y {
            let new_mat = factory.create_pbr_material_instance().unwrap();
            factory.set_material_data(
                &new_mat,
                &PbrMaterialData {
                    alpha_cutoff: 0.0,
                    color: Vec4::new(0.0, 0.0, 1.0, 1.0),
                    metallic: y as f32 / (SPHERE_Y as f32 - 1.0),
                    roughness: x as f32 / (SPHERE_X as f32 - 1.0),
                },
            );

            pack.2.push(Model(
                Mat4::from_translation(offset + (2.0 * Vec3::new(x as f32, y as f32, 0.0)))
                    * Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0)),
            ));
            pack.3.push(new_mat);
        }
    }

    app.world.entities().commands().create(pack, &mut []);

    /*
    app.world.entities().commands().create(
        (
            vec![Static(0); instance.meshes.meshes.len()],
            instance.meshes.meshes.clone(),
            instance.meshes.materials.clone(),
            instance.meshes.models.clone().into_iter().map(|src| Model(Mat4::from_translation(Vec3::new(-150.0, 0.0, 0.0)) * src.0)).collect(),
            instance.meshes.rendering_mode.clone(),
            instance.meshes.flags.clone(),
        ),
        &mut [],
    );

    app.world.entities().commands().create(
        (
            vec![Static(0); instance.meshes.meshes.len()],
            instance.meshes.meshes.clone(),
            instance.meshes.materials.clone(),
            instance.meshes.models.clone().into_iter().map(|src| Model(Mat4::from_translation(Vec3::new(150.0, 0.0, 0.0)) * src.0)).collect(),
            instance.meshes.rendering_mode.clone(),
            instance.meshes.flags.clone(),
        ),
        &mut [],
    );

    app.world.entities().commands().create(
        (
            vec![Static(0); instance.meshes.meshes.len()],
            instance.meshes.meshes.clone(),
            instance.meshes.materials.clone(),
            instance.meshes.models.clone().into_iter().map(|src| Model(Mat4::from_translation(Vec3::new(0.0, 0.0, 200.0)) * src.0)).collect(),
            instance.meshes.rendering_mode.clone(),
            instance.meshes.flags.clone(),
        ),
        &mut [],
    );

    app.world.entities().commands().create(
        (
            vec![Static(0); instance.meshes.meshes.len()],
            instance.meshes.meshes.clone(),
            instance.meshes.materials.clone(),
            instance.meshes.models.clone().into_iter().map(|src| Model(Mat4::from_translation(Vec3::new(0.0, 0.0, -200.0)) * src.0)).collect(),
            instance.meshes.rendering_mode.clone(),
            instance.meshes.flags.clone(),
        ),
        &mut [],
    );
    */

    // Big quad for the floor to prevent light leaking when the sun is low.
    let quad = factory
        .create_mesh(MeshCreateInfo {
            debug_name: Some("quad".to_owned()),
            vertices: VertexAttributes {
                positions: &[
                    Vec4::new(-1.0, 0.0, -1.0, 1.0),
                    Vec4::new(-1.0, 0.0, 1.0, 1.0),
                    Vec4::new(1.0, 0.0, 1.0, 1.0),
                    Vec4::new(1.0, 0.0, -1.0, 1.0),
                    Vec4::new(-1.0, 0.0, -1.0, 1.0),
                    Vec4::new(-1.0, 0.0, 1.0, 1.0),
                    Vec4::new(1.0, 0.0, 1.0, 1.0),
                    Vec4::new(1.0, 0.0, -1.0, 1.0),
                ],
                normals: &[
                    Vec4::new(0.0, 1.0, 0.0, 0.0),
                    Vec4::new(0.0, 1.0, 0.0, 0.0),
                    Vec4::new(0.0, 1.0, 0.0, 0.0),
                    Vec4::new(0.0, 1.0, 0.0, 0.0),
                    Vec4::new(0.0, -1.0, 0.0, 0.0),
                    Vec4::new(0.0, -1.0, 0.0, 0.0),
                    Vec4::new(0.0, -1.0, 0.0, 0.0),
                    Vec4::new(0.0, -1.0, 0.0, 0.0),
                ],
                tangents: None,
                colors: None,
                uv0: None,
                uv1: None,
                uv2: None,
                uv3: None,
            },
            indices: [1u32, 0, 2, 2, 0, 3, 4, 5, 6, 4, 6, 7].as_slice(),
        })
        .unwrap();

    let quad_material = factory.create_pbr_material_instance().unwrap();
    factory.set_material_data(
        &quad_material,
        &PbrMaterialData {
            alpha_cutoff: 0.0,
            color: Vec4::new(1.0, 1.0, 1.0, 1.0),
            metallic: 0.0,
            roughness: 1.0,
        },
    );

    app.world.entities().commands().create(
        (
            vec![quad.clone()],
            vec![quad_material.clone()],
            vec![Model(
                Mat4::from_scale(Vec3::new(1000.0, 1.0, 1000.0))
                    * Mat4::from_translation(Vec3::new(0.0, -3.0, 0.0)),
            )],
            vec![RenderingMode::Opaque],
            vec![RenderFlags::empty()],
            vec![Static(0)],
        ),
        &mut [],
    );

    // Create a mesh
    let mesh = factory
        .create_mesh(MeshCreateInfo {
            debug_name: Some("triangle".to_owned()),
            vertices: VertexAttributes {
                positions: &[
                    Vec4::new(1.0, 0.0, 0.1, 1.0),
                    Vec4::new(-1.0, 0.0, 0.1, 1.0),
                    Vec4::new(0.0, 1.0, 0.1, 1.0),
                ],
                normals: &[
                    Vec4::new(0.0, 0.0, 1.0, 0.0),
                    Vec4::new(0.0, 0.0, 1.0, 0.0),
                    Vec4::new(0.0, 0.0, 1.0, 0.0),
                ],
                tangents: None,
                colors: None,
                uv0: None,
                uv1: None,
                uv2: None,
                uv3: None,
            },
            indices: [0u32, 1, 2, 0, 2, 1].as_slice(),
        })
        .unwrap();

    // Create a material
    let material = factory.create_pbr_material_instance().unwrap();
    factory.set_material_data(
        &material,
        &PbrMaterialData {
            alpha_cutoff: 0.0,
            color: Vec4::new(1.0, 1.0, 1.0, 0.2),
            metallic: 0.0,
            roughness: 1.0,
        },
    );

    let red = factory.create_pbr_material_instance().unwrap();
    factory.set_material_data(
        &red,
        &PbrMaterialData {
            alpha_cutoff: 0.0,
            color: Vec4::new(1.0, 0.0, 0.0, 0.2),
            metallic: 0.0,
            roughness: 1.0,
        },
    );

    let green = factory.create_pbr_material_instance().unwrap();
    factory.set_material_data(
        &green, // purple = yes
        &PbrMaterialData {
            alpha_cutoff: 0.0,
            color: Vec4::new(0.0, 1.0, 0.0, 0.2),
            metallic: 0.0,
            roughness: 1.0,
        },
    );

    let blue = factory.create_pbr_material_instance().unwrap();
    factory.set_material_data(
        &blue,
        &PbrMaterialData {
            alpha_cutoff: 0.0,
            color: Vec4::new(0.0, 0.0, 1.0, 0.2),
            metallic: 0.0,
            roughness: 1.0,
        },
    );

    // Create a camera
    let mut camera = [Entity::null()];
    app.world.entities().commands().create(
        (
            vec![Camera {
                near: 0.03,
                far: 200.0,
                fov: 80.0_f32.to_radians(),
                order: 0,
                clear_color: CameraClearColor::Color(Vec4::ZERO),
                flags: RenderFlags::empty(),
            }],
            vec![Model(Mat4::from_translation(Vec3::new(0.0, 0.0, -5.0)))],
        ),
        &mut camera,
    );

    app.dispatcher.add_system(CameraMover {
        cursor_locked: false,
        look_speed: 0.1,
        move_speed: 24.0,
        entity: camera[0],
        position: Vec3::new(5.0, 5.0, 5.0),
        rotation: Vec3::ZERO,
    });

    // Create a renderable object
    app.world.entities().commands().create(
        (
            vec![mesh.clone()],
            vec![material.clone()],
            vec![Model(Mat4::IDENTITY)],
            vec![RenderingMode::Opaque],
            vec![RenderFlags::empty()],
            vec![Static(0)],
        ),
        &mut [],
    );

    app.world.entities().commands().create(
        (
            vec![mesh.clone()],
            vec![material.clone()],
            vec![Model(Mat4::from_translation(Vec3::new(0.0, 1.0, 0.0)))],
            vec![RenderingMode::AlphaCutout],
            vec![RenderFlags::empty()],
            vec![Static(0)],
        ),
        &mut [],
    );

    app.world.entities().commands().create(
        (
            vec![mesh.clone(), mesh.clone()],
            vec![red, green],
            vec![
                Model(Mat4::from_translation(Vec3::new(0.0, -1.0, 1.0))),
                Model(Mat4::from_translation(Vec3::new(0.0, -1.0, -1.0))),
            ],
            vec![RenderingMode::Transparent, RenderingMode::Transparent],
            vec![RenderFlags::empty(), RenderFlags::empty()],
            vec![Static(1), Static(1)],
        ),
        &mut [],
    );

    app.world.entities().commands().create(
        (
            vec![mesh.clone()],
            vec![blue],
            vec![Model(Mat4::from_translation(Vec3::new(0.0, -1.0, 0.0)))],
            vec![RenderingMode::Transparent],
            vec![RenderFlags::empty()],
        ),
        &mut [],
    );

    // Gimmie a ton of light
    use rand::prelude::*;

    const LIGHT_COUNT: usize = 0;
    const LIGHT_AREA_MIN: Vec3 = Vec3::new(-20.0, 0.0, -25.0);
    const LIGHT_AREA_MAX: Vec3 = Vec3::new(20.0, 30.0, 25.0);
    let mut rng = rand::thread_rng();
    let mut lights = (
        Vec::with_capacity(LIGHT_COUNT),
        Vec::with_capacity(LIGHT_COUNT),
    );

    for _ in 0..LIGHT_COUNT {
        lights.0.push(Light::Point {
            color: Vec3::new(
                (rng.gen::<f32>() * 0.5) + 0.5,
                (rng.gen::<f32>() * 0.5) + 0.5,
                (rng.gen::<f32>() * 0.5) + 0.5,
            ),
            range: 1.0,
            intensity: 64.0,
        });

        lights.1.push(Model(Mat4::from_translation(Vec3::new(
            rng.gen_range(LIGHT_AREA_MIN.x..LIGHT_AREA_MAX.x),
            rng.gen_range(LIGHT_AREA_MIN.y..LIGHT_AREA_MAX.y),
            rng.gen_range(LIGHT_AREA_MIN.z..LIGHT_AREA_MAX.z),
        ))));
    }

    app.world.entities().commands().create(lights, &mut []);

    app.resources.get_mut::<DirtyStatic>().unwrap().signal(0);
    app.resources.get_mut::<DirtyStatic>().unwrap().signal(1);
}
