/// This is the renderers "Shading Interface".
///
/// Essentially, this is a collection of all the types, constants, and descriptor set layouts and
/// bindings needed to communicate between the CPU and GPU.

pub const GLSL_INCLUDE_DIR: &'static str = concat!(env!("OUT_DIR"), "/glsl/");

pub mod consts {
    include!(concat!(env!("OUT_DIR"), "./gpu_consts.rs"));
}

pub mod types {
    use crate::consts::*;

    include!(concat!(env!("OUT_DIR"), "./gpu_types.rs"));

    /// Extracts the view frustum from a view * projection matrix.
    impl From<Mat4> for GpuFrustum {
        fn from(m: Mat4) -> Self {
            let mut frustum = GpuFrustum {
                planes: [
                    m.row(3) + m.row(0),
                    m.row(3) - m.row(0),
                    m.row(3) - m.row(1),
                    m.row(3) + m.row(1),
                    m.row(2),
                    m.row(3) - m.row(2),
                ],
            };

            for plane in &mut frustum.planes {
                *plane /= Vec4::new(plane.x, plane.y, plane.z, 0.0).length();
            }

            frustum
        }
    }
}

pub mod bindings {
    include!(concat!(env!("OUT_DIR"), "./gpu_bindings.rs"));
}
