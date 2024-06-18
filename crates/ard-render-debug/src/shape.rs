use ard_math::{Mat4, Vec3, Vec4};
use bytemuck::{Pod, Zeroable};

#[derive(Debug, Copy, Clone)]
pub enum Shape {
    Line {
        start: Vec3,
        end: Vec3,
    },
    Box {
        min_pt: Vec3,
        max_pt: Vec3,
        model: Mat4,
    },
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DebugShapeVertex {
    pub position: Vec4,
    pub color: Vec4,
}

unsafe impl Pod for DebugShapeVertex {}
unsafe impl Zeroable for DebugShapeVertex {}

impl Shape {
    #[inline(always)]
    pub fn vertex_count(&self) -> usize {
        match self {
            Shape::Line { .. } => 2,
            Shape::Box { .. } => 24,
        }
    }

    pub fn write_vertices(&self, dst: &mut [DebugShapeVertex], color: Vec4) {
        match self {
            Shape::Line { start, end } => {
                dst[0] = DebugShapeVertex {
                    position: Vec4::from((*start, 1.0)),
                    color,
                };
                dst[1] = DebugShapeVertex {
                    position: Vec4::from((*end, 1.0)),
                    color,
                };
            }
            Shape::Box {
                min_pt,
                max_pt,
                model,
            } => {
                let mut pts = [
                    Vec4::from((min_pt.x, min_pt.y, min_pt.z, 1.0)),
                    Vec4::from((max_pt.x, min_pt.y, min_pt.z, 1.0)),
                    Vec4::from((min_pt.x, min_pt.y, min_pt.z, 1.0)),
                    Vec4::from((min_pt.x, max_pt.y, min_pt.z, 1.0)),
                    Vec4::from((min_pt.x, min_pt.y, min_pt.z, 1.0)),
                    Vec4::from((min_pt.x, min_pt.y, max_pt.z, 1.0)),
                    Vec4::from((max_pt.x, max_pt.y, max_pt.z, 1.0)),
                    Vec4::from((min_pt.x, max_pt.y, max_pt.z, 1.0)),
                    Vec4::from((max_pt.x, max_pt.y, max_pt.z, 1.0)),
                    Vec4::from((max_pt.x, min_pt.y, max_pt.z, 1.0)),
                    Vec4::from((max_pt.x, max_pt.y, max_pt.z, 1.0)),
                    Vec4::from((max_pt.x, max_pt.y, min_pt.z, 1.0)),
                    Vec4::from((min_pt.x, max_pt.y, max_pt.z, 1.0)),
                    Vec4::from((min_pt.x, min_pt.y, max_pt.z, 1.0)),
                    Vec4::from((min_pt.x, max_pt.y, max_pt.z, 1.0)),
                    Vec4::from((min_pt.x, max_pt.y, min_pt.z, 1.0)),
                    Vec4::from((max_pt.x, min_pt.y, max_pt.z, 1.0)),
                    Vec4::from((min_pt.x, min_pt.y, max_pt.z, 1.0)),
                    Vec4::from((max_pt.x, min_pt.y, max_pt.z, 1.0)),
                    Vec4::from((max_pt.x, min_pt.y, min_pt.z, 1.0)),
                    Vec4::from((max_pt.x, max_pt.y, min_pt.z, 1.0)),
                    Vec4::from((min_pt.x, max_pt.y, min_pt.z, 1.0)),
                    Vec4::from((max_pt.x, max_pt.y, min_pt.z, 1.0)),
                    Vec4::from((max_pt.x, min_pt.y, min_pt.z, 1.0)),
                ];

                pts.iter_mut().enumerate().for_each(|(i, pt)| {
                    *pt = (*model) * (*pt);
                    *pt /= pt.w;
                    dst[i] = DebugShapeVertex {
                        position: *pt,
                        color,
                    };
                });
            }
        }
    }
}
