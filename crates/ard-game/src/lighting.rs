use ard_assets::{
    manager::Assets,
    prelude::{AssetNameBuf, Handle},
};
use ard_graphics_assets::prelude::CubeMapAsset;
use ard_math::Vec3;
use serde::{Deserialize, Serialize};

pub struct LightingSettings {
    pub ambient: Vec3,
    pub ambient_intensity: f32,
    pub sun_color: Vec3,
    pub sun_intensity: f32,
    pub sun_rotation: Vec3,
    pub skybox: Option<Handle<CubeMapAsset>>,
    pub irradiance: Option<Handle<CubeMapAsset>>,
    pub radiance: Option<Handle<CubeMapAsset>>,
}

#[derive(Serialize, Deserialize)]
pub struct LightingSettingsDescriptor {
    pub ambient: Vec3,
    pub ambient_intensity: f32,
    pub sun_color: Vec3,
    pub sun_intensity: f32,
    pub sun_rotation: Vec3,
    pub skybox: Option<AssetNameBuf>,
    pub irradiance: Option<AssetNameBuf>,
    pub radiance: Option<AssetNameBuf>,
}

impl LightingSettingsDescriptor {
    pub fn from_settings(settings: &LightingSettings, assets: &Assets) -> Self {
        Self {
            ambient: settings.ambient,
            ambient_intensity: settings.ambient_intensity,
            sun_color: settings.sun_color,
            sun_intensity: settings.sun_intensity,
            sun_rotation: settings.sun_rotation,
            skybox: settings
                .skybox
                .as_ref()
                .map(|handle| assets.get_name(handle)),
            irradiance: settings
                .irradiance
                .as_ref()
                .map(|handle| assets.get_name(handle)),
            radiance: settings
                .radiance
                .as_ref()
                .map(|handle| assets.get_name(handle)),
        }
    }

    pub fn into_settings(self, assets: &Assets) -> LightingSettings {
        LightingSettings {
            ambient: self.ambient,
            ambient_intensity: self.ambient_intensity,
            sun_color: self.sun_color,
            sun_intensity: self.sun_intensity,
            sun_rotation: self.sun_rotation,
            skybox: match self.skybox {
                Some(skybox) => assets.get_handle(&skybox),
                None => None,
            },
            irradiance: match self.irradiance {
                Some(irradiance) => assets.get_handle(&irradiance),
                None => None,
            },
            radiance: match self.radiance {
                Some(radiance) => assets.get_handle(&radiance),
                None => None,
            },
        }
    }
}

impl Default for LightingSettings {
    fn default() -> Self {
        Self {
            ambient: Vec3::ONE,
            ambient_intensity: 0.3,
            sun_color: Vec3::ONE,
            sun_intensity: 4.0,
            sun_rotation: Vec3::new((-45.0_f32).to_radians(), (-45.0_f32).to_radians(), 0.0),
            skybox: None,
            irradiance: None,
            radiance: None,
        }
    }
}

impl Default for LightingSettingsDescriptor {
    fn default() -> Self {
        Self {
            ambient: Vec3::ONE,
            ambient_intensity: 0.3,
            sun_color: Vec3::ONE,
            sun_intensity: 4.0,
            sun_rotation: Vec3::new((-45.0_f32).to_radians(), (-45.0_f32).to_radians(), 0.0),
            skybox: None,
            irradiance: None,
            radiance: None,
        }
    }
}
