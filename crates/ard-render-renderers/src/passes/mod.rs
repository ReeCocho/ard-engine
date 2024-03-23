use ard_render_material::factory::{MaterialFactory, PassDefinition, PassId};
use ard_render_si::bindings::Layouts;

pub mod color;
pub mod depth_only;
pub mod depth_prepass;

/// The depth only pass is used for high-z occlusion culling depth generation.
pub const HIGH_Z_PASS_ID: PassId = PassId::new(0);

/// Pass used for shadow mapping
pub const SHADOW_OPAQUE_PASS_ID: PassId = PassId::new(1);

/// Pass used for shadow mapping
pub const SHADOW_ALPHA_CUTOFF_PASS_ID: PassId = PassId::new(2);

/// The depth prepass results in a depth image containing opaque geometry.
pub const DEPTH_OPAQUE_PREPASS_PASS_ID: PassId = PassId::new(3);

/// The depth prepass results in a depth image containing opaque geometry.
pub const DEPTH_ALPHA_CUTOFF_PREPASS_PASS_ID: PassId = PassId::new(4);

/// The opaque pass renders only opaque geometry.
pub const COLOR_OPAQUE_PASS_ID: PassId = PassId::new(5);

/// The alpha-cutoff pass renders only opaque geometry.
pub const COLOR_ALPHA_CUTOFF_PASS_ID: PassId = PassId::new(6);

/// The transparent pass renders only transparent materials.
pub const TRANSPARENT_PASS_ID: PassId = PassId::new(7);

/// Defines primary passes.
pub fn define_passes<const FIF: usize>(factory: &mut MaterialFactory<FIF>, layouts: &Layouts) {
    factory
        .add_pass(
            HIGH_Z_PASS_ID,
            PassDefinition {
                layouts: vec![
                    layouts.depth_only_pass.clone(),
                    layouts.camera.clone(),
                    layouts.mesh_data.clone(),
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
            SHADOW_OPAQUE_PASS_ID,
            PassDefinition {
                layouts: vec![
                    layouts.depth_only_pass.clone(),
                    layouts.camera.clone(),
                    layouts.mesh_data.clone(),
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
            SHADOW_ALPHA_CUTOFF_PASS_ID,
            PassDefinition {
                layouts: vec![
                    layouts.depth_only_pass.clone(),
                    layouts.camera.clone(),
                    layouts.mesh_data.clone(),
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
            DEPTH_OPAQUE_PREPASS_PASS_ID,
            PassDefinition {
                layouts: vec![
                    layouts.depth_prepass.clone(),
                    layouts.camera.clone(),
                    layouts.mesh_data.clone(),
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
            DEPTH_ALPHA_CUTOFF_PREPASS_PASS_ID,
            PassDefinition {
                layouts: vec![
                    layouts.depth_prepass.clone(),
                    layouts.camera.clone(),
                    layouts.mesh_data.clone(),
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
            COLOR_OPAQUE_PASS_ID,
            PassDefinition {
                layouts: vec![
                    layouts.color_pass.clone(),
                    layouts.camera.clone(),
                    layouts.mesh_data.clone(),
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
            COLOR_ALPHA_CUTOFF_PASS_ID,
            PassDefinition {
                layouts: vec![
                    layouts.color_pass.clone(),
                    layouts.camera.clone(),
                    layouts.mesh_data.clone(),
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
                    layouts.color_pass.clone(),
                    layouts.camera.clone(),
                    layouts.mesh_data.clone(),
                    layouts.textures.clone(),
                    layouts.materials.clone(),
                ],
                has_depth_stencil_attachment: true,
                color_attachment_count: 1,
            },
        )
        .unwrap();
}
