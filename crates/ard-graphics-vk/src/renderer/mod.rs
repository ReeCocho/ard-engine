use std::time::{Duration, Instant};

use self::{
    debug_drawing::DebugDrawing,
    forward_plus::{ForwardPlus, GameRendererGraphRef},
    graph::RenderGraphContext,
    static_geometry::StaticGeometry,
};
use crate::{
    camera::{CameraUbo, DebugGui, Lighting},
    context::GraphicsContext,
    factory::Factory,
    shader_constants::MAX_SHADOW_CASCADES,
    surface::Surface,
    VkBackend,
};
use ard_core::core::{Stopping, Tick};
use ard_ecs::prelude::*;
use ard_graphics_api::prelude::*;
use ard_input::{InputState, Key, MouseButton};
use ard_math::{Mat4, Vec3};
use ard_render_graph::image::SizeGroup;
use ard_window::windows::Windows;
use ash::vk;

pub mod debug_drawing;
pub mod depth_pyramid;
pub mod forward_plus;
pub mod graph;
pub mod static_geometry;

/// Internal render event.
#[derive(Debug, Event, Copy, Clone)]
struct Render(Duration);

#[derive(SystemState)]
pub struct Renderer {
    ctx: GraphicsContext,
    factory: Factory,
    static_geometry: StaticGeometry,
    _debug_drawing: DebugDrawing,
    rg_ctx: RenderGraphContext<ForwardPlus>,
    state: ForwardPlus,
    graph: GameRendererGraphRef,
    last_render_time: Instant,
    canvas_size: Option<(u32, u32)>,
}

#[allow(clippy::type_complexity)]
type RenderResources = (
    Read<RendererSettings>,
    Write<Surface>,
    Read<Windows>,
    Write<Lighting>,
    Write<DebugGui>,
    Read<InputState>,
    // These three are held internally, but are requested so that no other systems write to them
    // while rendering is occuring.
    Write<Factory>,
    Write<StaticGeometry>,
    Write<DebugDrawing>,
);

impl RendererApi<VkBackend> for Renderer {
    fn new(
        create_info: &RendererCreateInfo<VkBackend>,
    ) -> (
        Self,
        Factory,
        StaticGeometry,
        DebugDrawing,
        DebugGui,
        Lighting,
    ) {
        let canvas_size = if let Some((width, height)) = create_info.settings.canvas_size {
            vk::Extent2D { width, height }
        } else {
            create_info.surface.0.lock().unwrap().resolution
        };

        let static_geometry = StaticGeometry::new();

        let mut rg_ctx = unsafe { RenderGraphContext::new(create_info.ctx) };

        let mut lighting = unsafe { Lighting::new(create_info.ctx) };

        let (graph, factory, debug_drawing, debug_gui, state) = unsafe {
            ForwardPlus::new_graph(
                create_info.ctx,
                create_info.surface,
                &mut rg_ctx,
                static_geometry.clone(),
                &lighting,
                create_info.settings.anisotropy_level,
                canvas_size,
                create_info.settings.render_scene,
            )
        };

        lighting.factory = Some(factory.clone());

        (
            Self {
                ctx: create_info.ctx.clone(),
                factory: factory.clone(),
                static_geometry: static_geometry.clone(),
                _debug_drawing: debug_drawing.clone(),
                last_render_time: Instant::now(),
                rg_ctx,
                graph,
                state,
                canvas_size: create_info.settings.canvas_size,
            },
            factory,
            static_geometry,
            debug_drawing,
            debug_gui,
            lighting,
        )
    }
}

impl Renderer {
    fn tick(
        &mut self,
        _: Tick,
        commands: Commands,
        _: Queries<()>,
        res: Res<(Read<RendererSettings>, Write<DebugGui>, Read<InputState>)>,
    ) {
        let res = res.get();
        let settings = res.0.unwrap();

        // See if rendering needs to be performed
        let now = Instant::now();
        let do_render = if let Some(render_time) = settings.render_time {
            now.duration_since(self.last_render_time) >= render_time
        } else {
            true
        };

        // Update input events
        if let Some(input) = res.2 {
            let mut debug_gui = res.1.unwrap();
            let mut io = debug_gui.context.io_mut();

            let scroll = input.mouse_scroll();
            io.mouse_wheel += scroll.1 as f32;
            io.mouse_wheel_h += scroll.0 as f32;

            io.mouse_down[0] = io.mouse_down[0] || input.mouse_button(MouseButton::Left);
            io.mouse_down[1] = io.mouse_down[1] || input.mouse_button(MouseButton::Right);
            io.mouse_down[2] = io.mouse_down[2] || input.mouse_button(MouseButton::Middle);

            io.keys_down[Key::Tab as usize] =
                io.keys_down[Key::Tab as usize] || input.key(Key::Tab);
            io.keys_down[Key::Right as usize] =
                io.keys_down[Key::Right as usize] || input.key(Key::Right);
            io.keys_down[Key::Left as usize] =
                io.keys_down[Key::Left as usize] || input.key(Key::Left);
            io.keys_down[Key::Up as usize] = io.keys_down[Key::Up as usize] || input.key(Key::Up);
            io.keys_down[Key::Down as usize] =
                io.keys_down[Key::Down as usize] || input.key(Key::Down);
            io.keys_down[Key::PageUp as usize] =
                io.keys_down[Key::PageUp as usize] || input.key(Key::PageUp);
            io.keys_down[Key::PageDown as usize] =
                io.keys_down[Key::PageDown as usize] || input.key(Key::PageDown);
            io.keys_down[Key::Home as usize] =
                io.keys_down[Key::Home as usize] || input.key(Key::Home);
            io.keys_down[Key::End as usize] =
                io.keys_down[Key::End as usize] || input.key(Key::End);
            io.keys_down[Key::Insert as usize] =
                io.keys_down[Key::Insert as usize] || input.key(Key::Insert);
            io.keys_down[Key::Delete as usize] =
                io.keys_down[Key::Delete as usize] || input.key(Key::Delete);
            io.keys_down[Key::Back as usize] =
                io.keys_down[Key::Back as usize] || input.key(Key::Back);
            io.keys_down[Key::Space as usize] =
                io.keys_down[Key::Space as usize] || input.key(Key::Space);
            io.keys_down[Key::Return as usize] =
                io.keys_down[Key::Return as usize] || input.key(Key::Return);
            io.keys_down[Key::Escape as usize] =
                io.keys_down[Key::Escape as usize] || input.key(Key::Escape);
            io.keys_down[Key::NumEnter as usize] =
                io.keys_down[Key::NumEnter as usize] || input.key(Key::NumEnter);
            io.keys_down[Key::A as usize] = io.keys_down[Key::A as usize] || input.key(Key::A);
            io.keys_down[Key::C as usize] = io.keys_down[Key::C as usize] || input.key(Key::C);
            io.keys_down[Key::V as usize] = io.keys_down[Key::V as usize] || input.key(Key::V);
            io.keys_down[Key::X as usize] = io.keys_down[Key::X as usize] || input.key(Key::X);
            io.keys_down[Key::Y as usize] = io.keys_down[Key::Y as usize] || input.key(Key::Y);
            io.keys_down[Key::Z as usize] = io.keys_down[Key::Z as usize] || input.key(Key::Z);

            io.key_shift = io.key_shift || input.key(Key::LShift) || input.key(Key::RShift);
            io.key_ctrl = io.key_ctrl || input.key(Key::LCtrl) || input.key(Key::RCtrl);
            io.key_alt = io.key_alt || input.key(Key::LAlt) || input.key(Key::RAlt);

            for c in input.input_string().chars() {
                io.add_input_character(c);
            }
        }

        // Send events
        if do_render {
            let dur = now.duration_since(self.last_render_time);
            self.last_render_time = now;
            commands.events.submit(PreRender(dur));
            commands.events.submit(Render(dur));
            commands.events.submit(PostRender(dur));
        }
    }

    fn stopping(&mut self, _: Stopping, _: Commands, _: Queries<()>, _: Res<()>) {
        unsafe {
            self.ctx.0.device.device_wait_idle().unwrap();
        }
    }

    unsafe fn render(
        &mut self,
        evt: Render,
        _: Commands,
        queries: Queries<(Read<Renderable<VkBackend>>, Read<PointLight>, Read<Model>)>,
        res: Res<RenderResources>,
    ) {
        let mut res = res.get();
        let settings = res.0.unwrap();
        let surface = res.1.unwrap();
        let mut surface_lock = surface.0.lock().expect("mutex poisoned");
        let windows = res.2.unwrap();
        let mut lighting = res.3.unwrap();
        let mut debug_gui = res.4.unwrap();

        let _static_geo_lock = self.static_geometry.acquire();
        let _factory_lock = self.factory.acquire();

        // Check if the window is minimized. If it is, we should skip rendering
        let window = windows
            .get(surface_lock.window)
            .expect("surface window is destroyed");
        if window.physical_height() == 0 || window.physical_width() == 0 {
            return;
        }

        // Move to next frame
        let frame_idx = self.rg_ctx.next_frame();

        // Acquire next image for presentation
        let (image_idx, image_available) = surface_lock.acquire_image();

        // Drop surface because the render graph needs it for presentation
        std::mem::drop(surface_lock);

        // Wait for rendering to finish
        self.state.wait(frame_idx);

        // Process pending resources
        self.factory.process(frame_idx);

        // If we have a custom canvas size, resize if it has changed
        if let Some(canvas_size) = &settings.canvas_size {
            let mut graph = self.graph.lock().expect("mutex poisoned");
            let size_group = graph
                .resources()
                .get_size_group(self.state.canvas_size_group());

            if size_group.width != canvas_size.0 || size_group.height != canvas_size.1 {
                // Wait for all graphics operations to complete so we are safe to resize
                self.ctx.0.device.device_wait_idle().unwrap();

                graph.update_size_group(
                    &mut self.rg_ctx,
                    self.state.canvas_size_group(),
                    SizeGroup {
                        width: canvas_size.0,
                        height: canvas_size.1,
                        mip_levels: 1,
                        array_layers: 1,
                    },
                );

                self.state.resize_canvas(graph.resources_mut());
            }
        }

        // Scene rendering check
        self.state.set_scene_render(settings.render_scene);

        // Update anisotropy setting if needed
        {
            let mut texture_sets = self.factory.0.texture_sets.lock().expect("mutex poisoned");
            if texture_sets.anisotropy() != settings.anisotropy_level {
                texture_sets.set_anisotropy(
                    settings.anisotropy_level,
                    &self.factory.0.textures.read().expect("mutex poisoned"),
                );
            }
        }

        // Update lighting. Compute projection matrix slices for shadow cascades
        let (vp_invs, far_planes) = {
            let graph = self.graph.lock().unwrap();
            let canvas_size = graph
                .resources()
                .get_size_group(self.state.surface_size_group());
            let cameras = self.factory.0.cameras.read().unwrap();
            let camera = cameras.get(self.factory.main_camera().id).unwrap();

            let view = Mat4::look_at_lh(
                camera.descriptor.position,
                camera.descriptor.center,
                camera.descriptor.up.try_normalize().unwrap_or(Vec3::Y),
            );

            let aspect_ratio = canvas_size.width as f32 / canvas_size.height as f32;
            let fmn = camera.descriptor.far - camera.descriptor.near;
            let mut projs = [Mat4::IDENTITY; MAX_SHADOW_CASCADES];
            let mut far_planes = [0.0; MAX_SHADOW_CASCADES];

            for i in 0..MAX_SHADOW_CASCADES {
                let lin_n = (i as f32 / MAX_SHADOW_CASCADES as f32).powf(2.0);
                let lin_f = ((i + 1) as f32 / MAX_SHADOW_CASCADES as f32).powf(2.0);

                far_planes[i] = camera.descriptor.near + (fmn * lin_f);

                let proj = Mat4::perspective_lh(
                    camera.descriptor.fov,
                    aspect_ratio,
                    camera.descriptor.near + (fmn * lin_n),
                    far_planes[i],
                );

                projs[i] = (proj * view).inverse();
            }

            (projs, far_planes)
        };

        let light_cameras = lighting.update_ubo(frame_idx, &vp_invs, &far_planes);

        // Update IMGUI data
        {
            let graph = self.graph.lock().unwrap();
            let canvas_size = graph
                .resources()
                .get_size_group(self.state.surface_size_group());
            let io = debug_gui.context.io_mut();
            io.delta_time = evt.0.as_secs_f32();
            io.display_size = [canvas_size.width as f32, canvas_size.height as f32];

            // Update keys if input exists
            if let Some(input) = res.5.as_mut() {
                let mouse_pos = input.mouse_pos();
                io.mouse_pos = [mouse_pos.0 as f32, mouse_pos.1 as f32];
            }
        }

        // Update context with outside state
        if let Some(draw_data) = debug_gui.finish_draw() {
            self.state.set_gui_draw_data(frame_idx, draw_data);
        }

        if let Some(skybox) = &lighting.skybox {
            let cube_maps = self.factory.0.cube_maps.read().unwrap();
            let mut texture_sets = self.factory.0.texture_sets.lock().unwrap();
            let cube_map = cube_maps.get(skybox.id).unwrap();
            let sampler = texture_sets.get_sampler(&cube_map.sampler);
            self.state.set_skybox_texture(frame_idx, cube_map, sampler);
        }

        if let Some(irradiance) = &lighting.irradiance {
            let cube_maps = self.factory.0.cube_maps.read().unwrap();
            let mut texture_sets = self.factory.0.texture_sets.lock().unwrap();
            let cube_map = cube_maps.get(irradiance.id).unwrap();
            let sampler = texture_sets.get_sampler(&cube_map.sampler);
            self.state
                .set_irradiance_texture(frame_idx, cube_map, sampler);
        }

        if let Some(radiance) = &lighting.radiance {
            let cube_maps = self.factory.0.cube_maps.read().unwrap();
            let mut texture_sets = self.factory.0.texture_sets.lock().unwrap();
            let cube_map = cube_maps.get(radiance.id).unwrap();
            let sampler = texture_sets.get_sampler(&cube_map.sampler);
            self.state
                .set_radiance_texture(frame_idx, cube_map, sampler);
        }

        self.state.set_sun_cameras(&light_cameras);
        self.state.set_dynamic_geo(&queries);
        self.state
            .set_point_light_query(queries.make::<(Read<PointLight>, Read<Model>)>());
        self.state.set_surface_image_idx(image_idx);

        // Run the render graph
        let commands = self.rg_ctx.command_buffer();
        self.graph.lock().expect("mutex poisoned").run(
            &mut self.rg_ctx,
            &mut self.state,
            &commands,
        );

        // Submit commands
        let frame = &self.state.frames()[frame_idx];

        let main_cb = [self.rg_ctx.command_buffer()];
        let main_signals = [frame.main_semaphore];
        let main_waits = [image_available];
        let main_wait_stgs = [vk::PipelineStageFlags::TRANSFER];

        let submit_info = [vk::SubmitInfo::builder()
            .command_buffers(&main_cb)
            .signal_semaphores(&main_signals)
            .wait_semaphores(&main_waits)
            .wait_dst_stage_mask(&main_wait_stgs)
            .build()];

        self.ctx
            .0
            .device
            .queue_submit(self.ctx.0.main, &submit_info, frame.fence)
            .expect("unable to submit rendering commands");

        let graphics_signals = [frame.main_semaphore];

        // Submit image to the screen
        let mut surface_lock = surface.0.lock().expect("mutex poisoned");
        if surface_lock.present(image_idx, &graphics_signals, &windows)
            && self.canvas_size.is_none()
        {
            let mut graph = self.graph.lock().expect("mutex poisoned");
            let resolution = surface_lock.resolution;

            // Surface was invalidated. If we have a surface depenent resolution, regenerate
            // the frames. No need to wait since a wait is performed if the surface is
            // invalidated.
            if self.canvas_size.is_none() {
                graph.update_size_group(
                    &mut self.rg_ctx,
                    self.state.canvas_size_group(),
                    SizeGroup {
                        width: resolution.width,
                        height: resolution.height,
                        mip_levels: 1,
                        array_layers: 1,
                    },
                );

                self.state.resize_canvas(graph.resources_mut());
            }

            // Notify canvas size group always
            graph.update_size_group(
                &mut self.rg_ctx,
                self.state.surface_size_group(),
                SizeGroup {
                    width: resolution.width,
                    height: resolution.height,
                    mip_levels: 1,
                    array_layers: 1,
                },
            );
        }

        // Reset imgui input state
        let mut io = debug_gui.context.io_mut();

        io.mouse_down[0] = false;
        io.mouse_down[1] = false;
        io.mouse_down[2] = false;

        io.keys_down[Key::Tab as usize] = false;
        io.keys_down[Key::Right as usize] = false;
        io.keys_down[Key::Left as usize] = false;
        io.keys_down[Key::Up as usize] = false;
        io.keys_down[Key::Down as usize] = false;
        io.keys_down[Key::PageUp as usize] = false;
        io.keys_down[Key::PageDown as usize] = false;
        io.keys_down[Key::Home as usize] = false;
        io.keys_down[Key::End as usize] = false;
        io.keys_down[Key::Insert as usize] = false;
        io.keys_down[Key::Delete as usize] = false;
        io.keys_down[Key::Back as usize] = false;
        io.keys_down[Key::Space as usize] = false;
        io.keys_down[Key::Return as usize] = false;
        io.keys_down[Key::Escape as usize] = false;
        io.keys_down[Key::NumEnter as usize] = false;
        io.keys_down[Key::A as usize] = false;
        io.keys_down[Key::C as usize] = false;
        io.keys_down[Key::V as usize] = false;
        io.keys_down[Key::X as usize] = false;
        io.keys_down[Key::Y as usize] = false;
        io.keys_down[Key::Z as usize] = false;

        io.key_shift = false;
        io.key_ctrl = false;
        io.key_alt = false;
    }
}

#[allow(clippy::from_over_into)]
impl Into<System> for Renderer {
    fn into(self) -> System {
        SystemBuilder::new(self)
            .with_handler(Renderer::tick)
            .with_handler(|s, e, c, q, r| unsafe { Renderer::render(s, e, c, q, r) })
            .with_handler(Renderer::stopping)
            .build()
    }
}
