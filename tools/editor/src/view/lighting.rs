use ard_engine::{graphics::prelude::*, math::*};

use super::View;

pub struct LightingGui;

impl View for LightingGui {
    fn show(
        &mut self,
        ui: &imgui::Ui,
        _: &mut crate::controller::Controller,
        resc: &mut crate::editor::Resources,
    ) {
        let settings = resc.scene_graph.lighting_mut();

        ui.window("Lighting Settings").build(|| {
            let mut ambient: [f32; 3] = settings.ambient.into();
            let mut sun_color: [f32; 3] = settings.sun_color.into();
            let mut sun_rot: [f32; 3] = settings.sun_rotation.into();
            for v in &mut sun_rot {
                *v *= 180.0 / std::f32::consts::PI;
            }

            ui.input_float3("Ambient Color", &mut ambient).build();
            ui.input_float("Ambient Intensity", &mut settings.ambient_intensity)
                .build();

            ui.input_float3("Sun Color", &mut sun_color).build();
            ui.input_float("Sun Intensity", &mut settings.sun_intensity)
                .build();

            ui.input_float3("Sun Rotation", &mut sun_rot).build();

            for v in &mut sun_rot {
                *v *= std::f32::consts::PI / 180.0;
            }

            settings.ambient = ambient.into();
            settings.sun_color = sun_color.into();
            settings.sun_rotation = sun_rot.into();
        });

        resc.lighting
            .set_ambient(settings.ambient, settings.ambient_intensity);
        resc.lighting
            .set_sun_color(settings.sun_color, settings.sun_intensity);
        resc.lighting.set_sun_direction(Vec3::new(
            settings.sun_rotation.x.cos() * settings.sun_rotation.y.cos(),
            settings.sun_rotation.x.sin() * settings.sun_rotation.y.cos(),
            settings.sun_rotation.y.sin(),
        ));
        resc.lighting.set_skybox_texture(match &settings.skybox {
            Some(skybox) => resc
                .assets
                .get(skybox)
                .map(|handle| handle.cube_map.clone()),
            None => None,
        });
        resc.lighting
            .set_radiance_texture(match &settings.radiance {
                Some(radiance) => resc
                    .assets
                    .get(radiance)
                    .map(|handle| handle.cube_map.clone()),
                None => None,
            });
        resc.lighting
            .set_irradiance_texture(match &settings.irradiance {
                Some(irradiance) => resc
                    .assets
                    .get(irradiance)
                    .map(|handle| handle.cube_map.clone()),
                None => None,
            });
    }
}
