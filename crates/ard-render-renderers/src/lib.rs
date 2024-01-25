use ard_render_material::factory::{MaterialFactory, PassDefinition, PassId};
use ard_render_si::bindings::Layouts;

pub mod bins;
pub mod calls;
pub mod draw_gen;
pub mod global;
pub mod highz;
pub mod scene;
pub mod shadow;
pub mod state;

/// The depth prepass results in a depth image containing opaque geometry and an image containing
/// normals for all geometry.
pub const DEPTH_PREPASS_PASS_ID: PassId = PassId::new(0);

/// The opaque pass renders only opaque geometry.
pub const OPAQUE_PASS_ID: PassId = PassId::new(1);

/// The transparent pass renders only transparent materials.
pub const TRANSPARENT_PASS_ID: PassId = PassId::new(2);

/// The depth only pass is used for high-z occlusion culling depth generation.
pub const HIGH_Z_PASS_ID: PassId = PassId::new(3);

/// Pass used for shadow mapping
pub const SHADOW_PASS_ID: PassId = PassId::new(4);

/// Defines primary passes.
pub fn define_passes<const FIF: usize>(factory: &mut MaterialFactory<FIF>, layouts: &Layouts) {
    factory
        .add_pass(
            DEPTH_PREPASS_PASS_ID,
            PassDefinition {
                layouts: vec![
                    layouts.global.clone(),
                    layouts.camera.clone(),
                    layouts.textures.clone(),
                    layouts.materials.clone(),
                ],
                has_depth_stencil_attachment: true,
                color_attachment_count: 0,
            },
        )
        .unwrap();

    factory
        .add_pass(
            SHADOW_PASS_ID,
            PassDefinition {
                layouts: vec![
                    layouts.global.clone(),
                    layouts.camera.clone(),
                    layouts.textures.clone(),
                    layouts.materials.clone(),
                ],
                has_depth_stencil_attachment: true,
                color_attachment_count: 0,
            },
        )
        .unwrap();

    factory
        .add_pass(
            HIGH_Z_PASS_ID,
            PassDefinition {
                layouts: vec![
                    layouts.global.clone(),
                    layouts.camera.clone(),
                    layouts.textures.clone(),
                    layouts.materials.clone(),
                ],
                has_depth_stencil_attachment: true,
                color_attachment_count: 0,
            },
        )
        .unwrap();

    factory
        .add_pass(
            OPAQUE_PASS_ID,
            PassDefinition {
                layouts: vec![
                    layouts.global.clone(),
                    layouts.camera.clone(),
                    layouts.textures.clone(),
                    layouts.materials.clone(),
                ],
                has_depth_stencil_attachment: true,
                color_attachment_count: 1,
            },
        )
        .unwrap();

    factory
        .add_pass(
            TRANSPARENT_PASS_ID,
            PassDefinition {
                layouts: vec![
                    layouts.global.clone(),
                    layouts.camera.clone(),
                    layouts.textures.clone(),
                    layouts.materials.clone(),
                ],
                has_depth_stencil_attachment: true,
                color_attachment_count: 1,
            },
        )
        .unwrap();
}
