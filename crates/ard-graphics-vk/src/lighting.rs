use ard_ecs::prelude::*;
use ard_graphics_api::prelude::*;
use ard_math::{Mat4, Vec2, Vec3, Vec4, Vec4Swizzles};
use bytemuck::{Pod, Zeroable};

use crate::{
    alloc::UniformBuffer,
    camera::{CameraUbo, CubeMap, Factory, GraphicsContext, Texture},
    shader_constants::MAX_SHADOW_CASCADES,
    VkBackend,
};

#[derive(Resource)]
pub struct Lighting {
    /// Contains the data to be sent to the actual UBO.
    pub(crate) ubo_data: LightingUbo,
    /// Shared UBO containing lighting information.
    pub(crate) ubo: UniformBuffer,
    pub(crate) factory: Option<Factory>,
    /// Skybox texture.
    pub(crate) skybox: Option<CubeMap>,
    pub(crate) irradiance: Option<CubeMap>,
    pub(crate) radiance: Option<CubeMap>,
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub(crate) struct RawPointLight {
    /// Color is `(x, y, z)` and `w` is intensity.
    pub color_intensity: Vec4,
    /// Position is `(x, y, z)` and `w` is range.
    pub position_range: Vec4,
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub(crate) struct LightingUbo {
    pub cascades: [ShadowCascadeInfo; MAX_SHADOW_CASCADES],
    pub ambient: Vec4,
    pub sun_color_intensity: Vec4,
    pub sun_direction: Vec4,
    pub sun_size: f32,
    pub radiance_mip_count: u32,
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

impl Default for LightingUbo {
    fn default() -> Self {
        Self {
            cascades: [ShadowCascadeInfo::default(); MAX_SHADOW_CASCADES],
            ambient: Vec4::ONE,
            sun_color_intensity: Vec4::new(1.0, 1.0, 1.0, 2.0),
            sun_direction: Vec4::new(1.0, -1.0, 1.0, 0.0).normalize(),
            sun_size: 0.5,
            radiance_mip_count: 0,
        }
    }
}

impl Default for ShadowCascadeInfo {
    fn default() -> Self {
        Self {
            vp: Mat4::IDENTITY,
            view: Mat4::IDENTITY,
            proj: Mat4::IDENTITY,
            uv_size: Vec2::ONE,
            far_plane: 0.0,
            min_bias: 0.2,
            max_bias: 0.5,
            depth_range: 1.0,
        }
    }
}

impl Lighting {
    pub(crate) unsafe fn new(ctx: &GraphicsContext) -> Self {
        let ubo = UniformBuffer::new(ctx, LightingUbo::default());

        Self {
            ubo,
            ubo_data: LightingUbo::default(),
            skybox: None,
            irradiance: None,
            radiance: None,
            factory: None,
        }
    }

    /// Updates the lighting UBO for a particular frame.
    ///
    /// Takes in a slice of inverse view-projection matrices that represent the camera to fit the
    /// light frustums around.
    ///
    /// Returns a set of `CameraUbo`s representing the light cameras.
    #[inline]
    pub(crate) unsafe fn update_ubo(
        &mut self,
        frame: usize,
        vp_invs: &[Mat4],
        far_planes: &[f32],
    ) -> [CameraUbo; MAX_SHADOW_CASCADES] {
        assert_eq!(vp_invs.len(), MAX_SHADOW_CASCADES);
        assert_eq!(far_planes.len(), MAX_SHADOW_CASCADES);

        let mut camera_ubos = [CameraUbo::default(); MAX_SHADOW_CASCADES];

        for i in 0..MAX_SHADOW_CASCADES {
            // Compute the corners of the view frustum in world space.
            let mut corners = [Vec4::ZERO; 8];
            for x in 0..2 {
                for y in 0..2 {
                    for z in 0..2 {
                        let pt = vp_invs[i]
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
            let view =
                Mat4::look_at_lh(center, center + self.ubo_data.sun_direction.xyz(), Vec3::Y);

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

            // Compute the vp matrix and update frustum size
            let mut cascade = &mut self.ubo_data.cascades[i];
            cascade.view = view;
            cascade.proj = proj;
            cascade.vp = proj * view;
            cascade.uv_size = self.ubo_data.sun_size / Vec2::new(max.x - min.x, max.y - min.y);
            cascade.far_plane = far_planes[i];
            cascade.depth_range = max.z - min.z;

            // Construct the frustum planes for culling. We set the back plane to 0 so that we
            // never cull objects behind the view.
            let mut frustum = Frustum::from(cascade.vp);
            frustum.planes[4] = Vec4::ZERO;

            camera_ubos[i] = CameraUbo {
                view,
                projection: proj,
                vp: cascade.vp,
                view_inv: view.inverse(),
                projection_inv: proj.inverse(),
                vp_inv: cascade.vp.inverse(),
                frustum,
                position: Vec4::from((center - (min.z * self.ubo_data.sun_direction.xyz()), 1.0)),
                cluster_scale_bias: Vec2::ZERO,
                fov: 1.0,
                near_clip: 1.0,
                far_clip: 1.0,
            };
        }

        // Write to the UBO
        self.ubo.write(self.ubo_data, frame);

        camera_ubos
    }
}

impl LightingApi<VkBackend> for Lighting {
    #[inline]
    fn set_ambient(&mut self, color: Vec3, intensity: f32) {
        self.ubo_data.ambient = Vec4::from((color, intensity));
    }

    #[inline]
    fn set_sun_color(&mut self, color: Vec3, intensity: f32) {
        self.ubo_data.sun_color_intensity = Vec4::from((color, intensity));
    }

    #[inline]
    fn set_sun_direction(&mut self, dir: Vec3) {
        self.ubo_data.sun_direction = Vec4::from((dir.normalize(), 1.0));
    }

    #[inline]
    fn set_skybox_texture(&mut self, texture: Option<CubeMap>) {
        self.skybox = texture;
    }

    #[inline]
    fn set_irradiance_texture(&mut self, texture: Option<CubeMap>) {
        self.irradiance = texture;
    }

    #[inline]
    fn set_radiance_texture(&mut self, texture: Option<CubeMap>) {
        match texture {
            Some(texture) => {
                self.ubo_data.radiance_mip_count = self
                    .factory
                    .as_ref()
                    .unwrap()
                    .0
                    .cube_maps
                    .read()
                    .unwrap()
                    .get(texture.id)
                    .unwrap()
                    .loaded_mips;

                self.radiance = Some(texture);
            }
            None => {
                self.radiance = None;
                self.ubo_data.radiance_mip_count = 0;
            }
        }
    }
}

unsafe impl Pod for RawPointLight {}
unsafe impl Zeroable for RawPointLight {}

unsafe impl Pod for LightingUbo {}
unsafe impl Zeroable for LightingUbo {}
