use std::num::NonZeroUsize;

use ard_math::{Mat4, Vec3, Vec4, Vec4Swizzles};
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
    Sphere {
        radius: f32,
        model: Mat4,
        segments: NonZeroUsize,
    },
    Cylinder {
        radius: f32,
        height: f32,
        model: Mat4,
        segments: NonZeroUsize,
    },
    Cone {
        radius: f32,
        height: f32,
        model: Mat4,
        segments: NonZeroUsize,
    },
    Capsule {
        radius: f32,
        height: f32,
        model: Mat4,
        segments: NonZeroUsize,
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
            Shape::Sphere { segments, .. } => segments.get() * 6,
            Shape::Cylinder { segments, .. } => (segments.get() * 4) + 8,
            Shape::Cone { segments, .. } => (segments.get() * 2) + 8,
            // First term is the two rings.
            // Second term is the four half circles.
            // Third term is the height bars.
            Shape::Capsule { segments, .. } => {
                (segments.get() * 4) + (((segments.get() + 1) / 2) * 8) + 8
            }
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
            Shape::Sphere {
                radius,
                model,
                segments,
            } => {
                let segments_per_ring = segments.get() * 2;

                let mut base = 0;
                let mut end = segments_per_ring;
                write_circle_segments(&mut dst[base..end], color, segments.get(), 0, 1, 0.0, 1.0);

                base += segments_per_ring;
                end += segments_per_ring;
                write_circle_segments(&mut dst[base..end], color, segments.get(), 0, 2, 0.0, 1.0);

                base += segments_per_ring;
                end += segments_per_ring;
                write_circle_segments(&mut dst[base..end], color, segments.get(), 1, 2, 0.0, 1.0);

                // Scale each point by the radius and then transform into world space
                for pt in dst {
                    pt.position = Vec4::from((*radius * pt.position.xyz(), 1.0));
                    pt.position = *model * pt.position;
                    pt.position /= pt.position.w;
                }
            }
            Shape::Cylinder {
                radius,
                height,
                model,
                segments,
            } => {
                let segments_per_ring = segments.get() * 2;
                let half_height = *height * 0.5;

                dst[0] = DebugShapeVertex {
                    color,
                    position: Vec4::new(-*radius, -half_height, 0.0, 1.0),
                };
                dst[1] = DebugShapeVertex {
                    color,
                    position: Vec4::new(-*radius, half_height, 0.0, 1.0),
                };

                dst[2] = DebugShapeVertex {
                    color,
                    position: Vec4::new(*radius, -half_height, 0.0, 1.0),
                };
                dst[3] = DebugShapeVertex {
                    color,
                    position: Vec4::new(*radius, half_height, 0.0, 1.0),
                };

                dst[4] = DebugShapeVertex {
                    color,
                    position: Vec4::new(0.0, -half_height, -*radius, 1.0),
                };
                dst[5] = DebugShapeVertex {
                    color,
                    position: Vec4::new(0.0, half_height, -*radius, 1.0),
                };

                dst[6] = DebugShapeVertex {
                    color,
                    position: Vec4::new(0.0, -half_height, *radius, 1.0),
                };
                dst[7] = DebugShapeVertex {
                    color,
                    position: Vec4::new(0.0, half_height, *radius, 1.0),
                };

                for pt in &mut dst[0..8] {
                    pt.position = *model * pt.position;
                    pt.position /= pt.position.w;
                }

                let mut base = 8;
                let mut end = base + segments_per_ring;
                write_circle_segments(&mut dst[base..end], color, segments.get(), 0, 2, 0.0, 1.0);

                for pt in &mut dst[base..end] {
                    pt.position = Vec4::from((*radius * pt.position.xyz(), 1.0));
                    pt.position = *model
                        * Mat4::from_translation(Vec3::new(0.0, half_height, 0.0))
                        * pt.position;
                    pt.position /= pt.position.w;
                }

                base += segments_per_ring;
                end += segments_per_ring;
                write_circle_segments(&mut dst[base..end], color, segments.get(), 0, 2, 0.0, 1.0);

                for pt in &mut dst[base..end] {
                    pt.position = Vec4::from((*radius * pt.position.xyz(), 1.0));
                    pt.position = *model
                        * Mat4::from_translation(Vec3::new(0.0, -half_height, 0.0))
                        * pt.position;
                    pt.position /= pt.position.w;
                }
            }
            Shape::Cone {
                radius,
                height,
                model,
                segments,
            } => {
                let segments_per_ring = segments.get() * 2;
                let half_height = *height * 0.5;

                dst[0] = DebugShapeVertex {
                    color,
                    position: Vec4::new(-*radius, -half_height, 0.0, 1.0),
                };
                dst[1] = DebugShapeVertex {
                    color,
                    position: Vec4::new(0.0, half_height, 0.0, 1.0),
                };

                dst[2] = DebugShapeVertex {
                    color,
                    position: Vec4::new(*radius, -half_height, 0.0, 1.0),
                };
                dst[3] = DebugShapeVertex {
                    color,
                    position: Vec4::new(0.0, half_height, 0.0, 1.0),
                };

                dst[4] = DebugShapeVertex {
                    color,
                    position: Vec4::new(0.0, -half_height, -*radius, 1.0),
                };
                dst[5] = DebugShapeVertex {
                    color,
                    position: Vec4::new(0.0, half_height, 0.0, 1.0),
                };

                dst[6] = DebugShapeVertex {
                    color,
                    position: Vec4::new(0.0, -half_height, *radius, 1.0),
                };
                dst[7] = DebugShapeVertex {
                    color,
                    position: Vec4::new(0.0, half_height, 0.0, 1.0),
                };

                for pt in &mut dst[0..8] {
                    pt.position = *model * pt.position;
                    pt.position /= pt.position.w;
                }

                let base = 8;
                let end = base + segments_per_ring;
                write_circle_segments(&mut dst[base..end], color, segments.get(), 0, 2, 0.0, 1.0);

                for pt in &mut dst[base..end] {
                    pt.position = Vec4::from((*radius * pt.position.xyz(), 1.0));
                    pt.position = *model
                        * Mat4::from_translation(Vec3::new(0.0, -half_height, 0.0))
                        * pt.position;
                    pt.position /= pt.position.w;
                }
            }
            Shape::Capsule {
                radius,
                height,
                model,
                segments,
            } => {
                let verts_per_ring = segments.get() * 2;
                let segments_per_half_ring = (segments.get() + 1) / 2;
                let verts_per_half_ring = segments_per_half_ring * 2;
                let half_height = *height * 0.5;

                dst[0] = DebugShapeVertex {
                    color,
                    position: Vec4::new(-*radius, -half_height, 0.0, 1.0),
                };
                dst[1] = DebugShapeVertex {
                    color,
                    position: Vec4::new(-*radius, half_height, 0.0, 1.0),
                };

                dst[2] = DebugShapeVertex {
                    color,
                    position: Vec4::new(*radius, -half_height, 0.0, 1.0),
                };
                dst[3] = DebugShapeVertex {
                    color,
                    position: Vec4::new(*radius, half_height, 0.0, 1.0),
                };

                dst[4] = DebugShapeVertex {
                    color,
                    position: Vec4::new(0.0, -half_height, -*radius, 1.0),
                };
                dst[5] = DebugShapeVertex {
                    color,
                    position: Vec4::new(0.0, half_height, -*radius, 1.0),
                };

                dst[6] = DebugShapeVertex {
                    color,
                    position: Vec4::new(0.0, -half_height, *radius, 1.0),
                };
                dst[7] = DebugShapeVertex {
                    color,
                    position: Vec4::new(0.0, half_height, *radius, 1.0),
                };

                for pt in &mut dst[0..8] {
                    pt.position = *model * pt.position;
                    pt.position /= pt.position.w;
                }

                let mut base = 8;
                let mut end = base + verts_per_ring;
                write_circle_segments(&mut dst[base..end], color, segments.get(), 0, 2, 0.0, 1.0);

                for pt in &mut dst[base..end] {
                    pt.position = Vec4::from((*radius * pt.position.xyz(), 1.0));
                    pt.position = *model
                        * Mat4::from_translation(Vec3::new(0.0, half_height, 0.0))
                        * pt.position;
                    pt.position /= pt.position.w;
                }

                base += verts_per_ring;
                end += verts_per_ring;
                write_circle_segments(&mut dst[base..end], color, segments.get(), 0, 2, 0.0, 1.0);

                for pt in &mut dst[base..end] {
                    pt.position = Vec4::from((*radius * pt.position.xyz(), 1.0));
                    pt.position = *model
                        * Mat4::from_translation(Vec3::new(0.0, -half_height, 0.0))
                        * pt.position;
                    pt.position /= pt.position.w;
                }

                base += verts_per_ring;
                end += verts_per_half_ring;
                write_circle_segments(
                    &mut dst[base..end],
                    color,
                    segments_per_half_ring,
                    0,
                    1,
                    0.0,
                    0.5,
                );

                for pt in &mut dst[base..end] {
                    pt.position = Vec4::from((*radius * pt.position.xyz(), 1.0));
                    pt.position = *model
                        * Mat4::from_translation(Vec3::new(0.0, half_height, 0.0))
                        * pt.position;
                    pt.position /= pt.position.w;
                }

                base += verts_per_half_ring;
                end += verts_per_half_ring;
                write_circle_segments(
                    &mut dst[base..end],
                    color,
                    segments_per_half_ring,
                    2,
                    1,
                    0.0,
                    0.5,
                );

                for pt in &mut dst[base..end] {
                    pt.position = Vec4::from((*radius * pt.position.xyz(), 1.0));
                    pt.position = *model
                        * Mat4::from_translation(Vec3::new(0.0, half_height, 0.0))
                        * pt.position;
                    pt.position /= pt.position.w;
                }

                base += verts_per_half_ring;
                end += verts_per_half_ring;
                write_circle_segments(
                    &mut dst[base..end],
                    color,
                    segments_per_half_ring,
                    0,
                    1,
                    0.5,
                    1.0,
                );

                for pt in &mut dst[base..end] {
                    pt.position = Vec4::from((*radius * pt.position.xyz(), 1.0));
                    pt.position = *model
                        * Mat4::from_translation(Vec3::new(0.0, -half_height, 0.0))
                        * pt.position;
                    pt.position /= pt.position.w;
                }

                base += verts_per_half_ring;
                end += verts_per_half_ring;
                write_circle_segments(
                    &mut dst[base..end],
                    color,
                    segments_per_half_ring,
                    2,
                    1,
                    0.5,
                    1.0,
                );

                for pt in &mut dst[base..end] {
                    pt.position = Vec4::from((*radius * pt.position.xyz(), 1.0));
                    pt.position = *model
                        * Mat4::from_translation(Vec3::new(0.0, -half_height, 0.0))
                        * pt.position;
                    pt.position /= pt.position.w;
                }
            }
        }
    }
}

fn write_circle_segments(
    dst: &mut [DebugShapeVertex],
    color: Vec4,
    segments: usize,
    dim1: usize,
    dim2: usize,
    tstart: f32,
    tend: f32,
) {
    for i in 0..segments {
        let mut t1 = i as f32 / segments as f32;
        let mut t2 = (i + 1) as f32 / segments as f32;
        t1 = (t1 * (tend - tstart)) + tstart;
        t2 = (t2 * (tend - tstart)) + tstart;
        t1 *= std::f32::consts::PI * 2.0;
        t2 *= std::f32::consts::PI * 2.0;

        dst[i * 2] = {
            let mut position = Vec4::new(0.0, 0.0, 0.0, 1.0);
            position[dim1] = t1.cos();
            position[dim2] = t1.sin();
            DebugShapeVertex { position, color }
        };

        dst[(i * 2) + 1] = {
            let mut position = Vec4::new(0.0, 0.0, 0.0, 1.0);
            position[dim1] = t2.cos();
            position[dim2] = t2.sin();
            DebugShapeVertex { position, color }
        };
    }
}
