use ard_math::Vec3;

#[derive(Debug, Copy, Clone, Default)]
pub struct CameraCreateInfo {
    pub descriptor: CameraDescriptor,
}

#[derive(Debug, Copy, Clone)]
pub struct CameraDescriptor {
    /// The global position of the camera.
    pub position: Vec3,
    /// The global position that the camera is looking at.
    pub center: Vec3,
    /// Vector pointing upwards relative to the camera.
    pub up: Vec3,
    /// Near clipping plane.
    pub near: f32,
    /// Far clipping plane.
    pub far: f32,
    /// Vertical field of view in radians.
    pub fov: f32,
}

pub trait CameraApi: Clone + Send + Sync {}

impl Default for CameraDescriptor {
    fn default() -> Self {
        CameraDescriptor {
            position: Vec3::ZERO,
            center: Vec3::Z,
            up: Vec3::Y,
            near: 0.03,
            far: 100.0,
            fov: 80.0 * (std::f32::consts::PI / 180.0),
        }
    }
}
