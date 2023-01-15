use std::ops::DerefMut;

use ard_ecs::prelude::*;
use ard_math::{Mat4, Vec2, Vec3, Vec4, Vec4Swizzles};
use ard_pal::prelude::*;
use bytemuck::{Pod, Zeroable};
use ordered_float::NotNan;

use crate::{
    camera::{CameraDescriptor, CameraIbl, CameraUbo, Frustum},
    cube_map::CubeMapInner,
    factory::{
        allocator::ResourceAllocator, materials::MaterialBuffers, meshes::MeshBuffers,
        textures::TextureSets, Factory, Layouts,
    },
    lighting::Lighting,
    material::{MaterialInner, PipelineType},
    mesh::MeshInner,
    shader_constants::{FRAMES_IN_FLIGHT, MAX_SHADOW_CASCADES},
    static_geometry::StaticGeometryInner,
};

use super::{
    render_data::{GlobalRenderData, RenderArgs, RenderData},
    RenderLayer,
};

const SHADOW_MAP_FORMAT: TextureFormat = TextureFormat::D32Sfloat;

pub(crate) const SHADOW_SAMPLER: Sampler = Sampler {
    min_filter: Filter::Linear,
    mag_filter: Filter::Linear,
    mipmap_filter: Filter::Nearest,
    address_u: SamplerAddressMode::ClampToBorder,
    address_v: SamplerAddressMode::ClampToBorder,
    address_w: SamplerAddressMode::ClampToBorder,
    anisotropy: None,
    compare: Some(CompareOp::Less),
    min_lod: unsafe { NotNan::new_unchecked(0.0) },
    max_lod: Some(unsafe { NotNan::new_unchecked(0.0) }),
    border_color: Some(BorderColor::FloatOpaqueWhite),
    unnormalize_coords: false,
};

pub(crate) struct Shadows {
    pub cascades: Vec<Cascade>,
    pub ubo: Buffer,
}

pub(crate) struct Cascade {
    pub map: Texture,
    pub render_data: RenderData,
    pub info: ShadowCascadeInfo,
}

pub(crate) struct ShadowRenderArgs<'a> {
    pub texture_sets: &'a TextureSets,
    pub material_buffers: &'a MaterialBuffers,
    pub mesh_buffers: &'a MeshBuffers,
    pub materials: &'a ResourceAllocator<MaterialInner>,
    pub meshes: &'a ResourceAllocator<MeshInner>,
    pub global: &'a GlobalRenderData,
}

pub(crate) struct ShadowPrepareArgs<'a> {
    pub frame: usize,
    pub lighting: &'a Lighting,
    pub queries: &'a Queries<Everything>,
    pub static_geometry: &'a StaticGeometryInner,
    pub factory: &'a Factory,
    pub camera: &'a CameraDescriptor,
    pub use_alternate: bool,
    pub lock_occlusion: bool,
    pub camera_dims: (f32, f32),
}

#[derive(Debug, Default, Copy, Clone)]
#[repr(C, align(16))]
pub(crate) struct ShadowInfo {
    pub cascades: [ShadowCascadeInfo; MAX_SHADOW_CASCADES],
    pub cascade_count: u32,
}

#[derive(Debug, Copy, Clone)]
#[repr(C, align(16))]
pub(crate) struct ShadowCascadeInfo {
    pub vp: Mat4,
    pub view: Mat4,
    pub proj: Mat4,
    pub uv_size: Vec2,
    pub far_plane: f32,
    pub min_bias: f32,
    pub max_bias: f32,
    pub depth_range: f32,
}

unsafe impl Pod for ShadowInfo {}
unsafe impl Zeroable for ShadowInfo {}

impl Shadows {
    pub fn new(
        ctx: &Context,
        layouts: &Layouts,
        shadow_map_resolution: u32,
        num_cascades: usize,
    ) -> Self {
        assert!(num_cascades <= MAX_SHADOW_CASCADES);

        // Create cascades
        let mut cascades = Vec::with_capacity(num_cascades);
        for i in 0..num_cascades {
            let name = format!("shadow_cascade_{i}");

            // Create shadow map
            let dim = shadow_map_resolution; // shadow_map_resolution.shr(i).max(1);
            let map = Texture::new(
                ctx.clone(),
                TextureCreateInfo {
                    format: SHADOW_MAP_FORMAT,
                    ty: TextureType::Type2D,
                    width: dim,
                    height: dim,
                    depth: 1,
                    array_elements: 1,
                    mip_levels: 1,
                    texture_usage: TextureUsage::DEPTH_STENCIL_ATTACHMENT | TextureUsage::SAMPLED,
                    memory_usage: MemoryUsage::GpuOnly,
                    debug_name: Some(name.clone()),
                },
            )
            .unwrap();

            // Create render data
            let render_data = RenderData::new(ctx, &name, layouts, false);

            cascades.push(Cascade {
                map,
                render_data,
                info: ShadowCascadeInfo::default(),
            })
        }

        // Create shadow info UBO
        let ubo = Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: std::mem::size_of::<ShadowInfo>() as u64,
                array_elements: FRAMES_IN_FLIGHT,
                buffer_usage: BufferUsage::UNIFORM_BUFFER,
                memory_usage: MemoryUsage::CpuToGpu,
                debug_name: Some(String::from("shadow_info_ubo")),
            },
        )
        .unwrap();

        Self { cascades, ubo }
    }

    /// Prepares the shadow map for rendering.
    ///
    /// Requires a camera to render the shadows for.
    pub fn prepare(&mut self, args: ShadowPrepareArgs) {
        let aspect_ratio = args.camera_dims.0 / args.camera_dims.1;
        let fmn = args.camera.far - args.camera.near;

        let mut info = ShadowInfo {
            cascade_count: self.cascades.len() as u32,
            ..Default::default()
        };

        // Prepare each cascade for rendering
        let cascade_count = self.cascades.len() as f32;
        for (i, cascade) in self.cascades.iter_mut().enumerate() {
            // Compute view and projection matrices for the camera at this slice of the frustum
            let view = Mat4::look_at_lh(
                args.camera.position,
                args.camera.target,
                args.camera.up.try_normalize().unwrap_or(Vec3::Y),
            );

            let lin_n = (i as f32 / cascade_count).powf(2.0);
            let lin_f = ((i + 1) as f32 / cascade_count).powf(2.0);
            let far_plane = args.camera.near + (fmn * lin_f);
            let proj = Mat4::perspective_lh(
                args.camera.fov,
                aspect_ratio,
                args.camera.near + (fmn * lin_n),
                far_plane,
            );

            let vp_inv = (proj * view).inverse();

            // Determine the position of the four corners of the frustum
            let mut corners = [Vec4::ZERO; 8];
            for x in 0..2 {
                for y in 0..2 {
                    for z in 0..2 {
                        let pt = vp_inv
                            * Vec4::new(2.0 * x as f32 - 1.0, 2.0 * y as f32 - 1.0, z as f32, 1.0);
                        corners[(x * 4) + (y * 2) + z] = pt / pt.w;
                    }
                }
            }

            // Compute the center of the frustum by averaging all the points.
            let mut center = Vec3::ZERO;
            for corner in &corners {
                center += corner.xyz();
            }
            center /= 8.0;

            // Compute the view matrix for the light
            let view = Mat4::look_at_lh(
                center,
                center + args.lighting.data.sun_direction.xyz(),
                Vec3::Y,
            );

            // We need to compute the left/right/top/bottom/near/far values for the ortho-projection
            // for the sun. To do this, we convert the corners of the view frustum in world space to
            // light space, and then take the min and max there.
            let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
            let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);

            for corner in &corners {
                let pt = view * (*corner);
                min = min.min(pt.xyz());
                max = max.max(pt.xyz());
            }

            // Bug fix for clipping when objects are behind the camera view
            min.z -= 1.0;

            let proj = Mat4::orthographic_lh(min.x, max.x, min.y, max.y, min.z, max.z);

            // Update cascade info in lighting
            cascade.info.view = view;
            cascade.info.proj = proj;
            cascade.info.vp = proj * view;
            cascade.info.uv_size =
                args.lighting.data.sun_size / Vec2::new(max.x - min.x, max.y - min.y);
            cascade.info.far_plane = far_plane;
            cascade.info.depth_range = max.z - min.z;
            info.cascades[i] = cascade.info;

            // Construct the frustum planes for culling. We set the back plane to 0 so that we
            // never cull objects behind the view.
            let mut frustum = Frustum::from(cascade.info.vp);
            frustum.planes[4] = Vec4::ZERO;

            // Update render data
            if !args.lock_occlusion {
                cascade.render_data.prepare_input_ids(
                    args.frame,
                    RenderLayer::SHADOW_CASTER,
                    args.queries,
                    args.static_geometry,
                );
                cascade.render_data.prepare_draw_calls(
                    args.frame,
                    args.use_alternate,
                    args.factory,
                );
            }

            cascade.render_data.update_camera_ubo(
                args.frame,
                CameraUbo {
                    view,
                    projection: proj,
                    vp: cascade.info.vp,
                    view_inv: view.inverse(),
                    projection_inv: proj.inverse(),
                    vp_inv: cascade.info.vp.inverse(),
                    frustum,
                    position: Vec4::from((
                        center - (min.z * args.lighting.data.sun_direction.xyz()),
                        1.0,
                    )),
                    cluster_scale_bias: Vec2::ZERO,
                    fov: 1.0,
                    near_clip: 1.0,
                    far_clip: 1.0,
                    pem_mip_count: 1,
                },
            );
        }

        // Update the shadow info UBO
        let mut view = self.ubo.write(args.frame).unwrap();
        bytemuck::cast_slice_mut::<_, ShadowInfo>(view.deref_mut())[0] = info;
    }

    /// Update sets for shadow mapping.
    pub fn update_sets(
        &mut self,
        frame: usize,
        global_data: &GlobalRenderData,
        lighting: &Lighting,
        cube_maps: &ResourceAllocator<CubeMapInner>,
        use_alternate: bool,
    ) {
        for cascade in &mut self.cascades {
            cascade
                .render_data
                .update_draw_gen_set(global_data, None, frame, use_alternate);
            cascade.render_data.update_global_set(
                global_data,
                lighting,
                &CameraIbl::default(),
                cube_maps,
                frame,
            );
            cascade
                .render_data
                .update_camera_with_shadows(frame, global_data, None);
        }
    }

    /// Render each cascade of the shadow map
    pub fn render<'a>(
        &'a self,
        frame: usize,
        use_alternate: bool,
        args: ShadowRenderArgs<'a>,
        commands: &mut CommandBuffer<'a>,
    ) {
        for cascade in &self.cascades {
            commands.render_pass(
                RenderPassDescriptor {
                    color_attachments: Vec::default(),
                    depth_stencil_attachment: Some(DepthStencilAttachment {
                        texture: &cascade.map,
                        array_element: 0,
                        mip_level: 0,
                        load_op: LoadOp::Clear(ClearColor::D32S32(1.0, 0)),
                        store_op: StoreOp::Store,
                    }),
                },
                |pass| {
                    let draw_count =
                        cascade.render_data.dynamic_draws + cascade.render_data.static_draws;
                    cascade.render_data.render(
                        frame,
                        use_alternate,
                        RenderArgs {
                            pass,
                            texture_sets: args.texture_sets,
                            material_buffers: args.material_buffers,
                            mesh_buffers: args.mesh_buffers,
                            materials: args.materials,
                            meshes: args.meshes,
                            global: args.global,
                            pipeline_ty: PipelineType::Shadow,
                            draw_offset: 0,
                            draw_count,
                            draw_sky_box: false,
                            material_override: None,
                        },
                    )
                },
            );
        }
    }

    /// Generates draw calls for each cascade.
    pub fn generate_draw_calls<'a>(
        &'a self,
        frame: usize,
        global: &GlobalRenderData,
        commands: &mut CommandBuffer<'a>,
    ) {
        for cascade in &self.cascades {
            cascade
                .render_data
                .generate_draw_calls(frame, global, false, Vec2::ONE, commands);
        }
    }
}

impl Default for ShadowCascadeInfo {
    fn default() -> Self {
        ShadowCascadeInfo {
            vp: Mat4::IDENTITY,
            view: Mat4::IDENTITY,
            proj: Mat4::IDENTITY,
            uv_size: Vec2::ONE,
            far_plane: 0.0,
            min_bias: 0.05,
            max_bias: 0.2,
            depth_range: 1.0,
        }
    }
}
