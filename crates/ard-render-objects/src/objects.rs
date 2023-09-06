use ard_core::prelude::{Disabled, Static};
use ard_ecs::{prelude::Entity, resource::Resource};
use ard_math::Vec3A;
use ard_pal::prelude::{Buffer, BufferCreateInfo, BufferUsage, Context, MemoryUsage};
use ard_render_base::ecs::Frame;
use ard_render_material::material_instance::MaterialInstance;
use ard_render_meshes::mesh::Mesh;

use crate::{keys::DrawKey, Model, RenderFlags, RenderingMode};
use ard_render_si::types::GpuObjectData;

pub const DEFAULT_OBJECT_DATA_CAP: usize = 1;

/// Flag used to indicate if static objects have been modified.
#[derive(Resource, Copy, Clone)]
pub struct StaticDirty(bool);

/// Contains a complete collection of objects to render.
pub struct RenderObjects {
    object_data: Buffer,
    static_objects: ObjectList,
    dynamic_objects: ObjectList,
    static_dirty: Vec<u32>,
}

#[derive(Default)]
pub struct ObjectList {
    pub opaque: Vec<OpaqueObjectIndex>,
    pub alpha_cutout: Vec<AlphaCutoutObjectIndex>,
    pub transparent: Vec<TransparentObjectIndex>,
}

pub struct OpaqueObjectIndex {
    pub key: DrawKey,
    pub flags: RenderFlags,
    pub idx: u32,
}

pub struct AlphaCutoutObjectIndex {
    pub key: DrawKey,
    pub flags: RenderFlags,
    pub idx: u32,
}

pub struct TransparentObjectIndex {
    pub key: DrawKey,
    pub flags: RenderFlags,
    pub idx: u32,
    pub position: Vec3A,
}

impl RenderObjects {
    pub fn new(ctx: Context, frames_in_flight: usize) -> Self {
        Self {
            object_data: Buffer::new(
                ctx,
                BufferCreateInfo {
                    size: (DEFAULT_OBJECT_DATA_CAP * std::mem::size_of::<GpuObjectData>()) as u64,
                    array_elements: frames_in_flight,
                    buffer_usage: BufferUsage::STORAGE_BUFFER,
                    memory_usage: MemoryUsage::CpuToGpu,
                    debug_name: Some("object_data".into()),
                },
            )
            .unwrap(),
            static_objects: ObjectList::default(),
            dynamic_objects: ObjectList::default(),
            static_dirty: (0..frames_in_flight).map(|_| 2).collect(),
        }
    }

    #[inline(always)]
    pub fn object_data(&self) -> &Buffer {
        &self.object_data
    }

    #[inline(always)]
    pub fn static_dirty(&self, frame: Frame) -> bool {
        self.static_dirty[usize::from(frame)] > 0
    }

    pub fn upload_objects<'a>(
        &mut self,
        frame: Frame,
        static_objs: impl ExactSizeIterator<
            Item = (
                Entity,
                (
                    &'a Mesh,
                    &'a MaterialInstance,
                    &'a Model,
                    &'a RenderingMode,
                    &'a RenderFlags,
                    &'a Static,
                ),
                Option<&'a Disabled>,
            ),
        >,
        dynamic_objs: impl ExactSizeIterator<
            Item = (
                Entity,
                (
                    &'a Mesh,
                    &'a MaterialInstance,
                    &'a Model,
                    &'a RenderingMode,
                    &'a RenderFlags,
                ),
                Option<&'a Disabled>,
            ),
        >,
        mut static_dirty: bool,
    ) {
        // Expand the object data buffer if required
        let static_obj_count = static_objs.len();
        let dynamic_obj_count = dynamic_objs.len();
        let obj_count = static_obj_count + dynamic_obj_count;
        let expanded = match Buffer::expand(
            &self.object_data,
            (obj_count * std::mem::size_of::<GpuObjectData>()) as u64,
            false,
        ) {
            Some(buffer) => {
                self.object_data = buffer;
                true
            }
            None => false,
        };

        // Update dirty flags
        if static_dirty || expanded {
            self.static_dirty.iter_mut().for_each(|f| *f = 2);
            static_dirty = true;
        } else {
            static_dirty = self.static_dirty(frame);
            self.static_dirty[usize::from(frame)] =
                self.static_dirty[usize::from(frame)].saturating_sub(1);
        }

        // Write in every object into the buffer
        let mut view = self.object_data.write(frame.into()).unwrap();
        let slice = bytemuck::cast_slice_mut::<_, GpuObjectData>(&mut view);
        let mut idx = 0;

        // Only need to write in static geometry if they have been modified, or if the object data
        // buffer expanded.
        if static_dirty || expanded {
            self.static_objects.clear();

            for (e, (mesh, mat, mdl, mode, flags, _), disabled) in static_objs {
                if disabled.is_some() {
                    continue;
                }

                Self::write_renderable(
                    idx as u32,
                    &mut slice[idx],
                    e,
                    (mesh, mat, mdl, mode, flags),
                    &mut self.static_objects,
                );

                idx += 1;
            }
        } else {
            idx += static_obj_count;
        }

        // Write in dynamic geometry
        self.dynamic_objects.clear();

        for (e, (mesh, mat, mdl, mode, flags), disabled) in dynamic_objs {
            if disabled.is_some() {
                continue;
            }

            Self::write_renderable(
                idx as u32,
                &mut slice[idx],
                e,
                (mesh, mat, mdl, mode, flags),
                &mut self.static_objects,
            );

            idx += 1;
        }
    }

    #[inline(always)]
    pub fn static_objects(&self) -> &ObjectList {
        &self.static_objects
    }

    #[inline(always)]
    pub fn dynamic_objects(&self) -> &ObjectList {
        &self.dynamic_objects
    }

    #[inline]
    fn write_renderable(
        idx: u32,
        data: &mut GpuObjectData,
        entity: Entity,
        query: (
            &Mesh,
            &MaterialInstance,
            &Model,
            &RenderingMode,
            &RenderFlags,
        ),
        list: &mut ObjectList,
    ) {
        let (mesh, mat, mdl, mode, flags) = query;

        // Write the object ID to the appropriate list based on the rendering mode
        match *mode {
            RenderingMode::Opaque => {
                list.opaque.push(OpaqueObjectIndex {
                    key: DrawKey::new(mat, mesh),
                    flags: *flags,
                    idx,
                });
            }
            RenderingMode::AlphaCutout => {
                list.alpha_cutout.push(AlphaCutoutObjectIndex {
                    key: DrawKey::new(mat, mesh),
                    flags: *flags,
                    idx,
                });
            }
            RenderingMode::Transparent => list.transparent.push(TransparentObjectIndex {
                key: DrawKey::new(mat, mesh),
                flags: *flags,
                idx,
                position: mdl.position(),
            }),
        }

        // Write in the object data
        *data = GpuObjectData {
            model: mdl.0,
            normal: mdl.0.inverse().transpose(),
            entity_id: entity.id(),
            entity_ver: entity.ver(),
            material: mat.data_slot().map(|slot| slot.into()).unwrap_or_default(),
            textures: mat.tex_slot().map(|slot| slot.into()).unwrap_or_default(),
        };
    }
}

impl StaticDirty {
    #[inline(always)]
    pub fn mark(&mut self) {
        self.0 = true;
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.0 = false;
    }
}

impl From<StaticDirty> for bool {
    fn from(value: StaticDirty) -> Self {
        value.0
    }
}

impl Default for StaticDirty {
    fn default() -> Self {
        Self(true)
    }
}

impl ObjectList {
    #[inline]
    pub fn clear(&mut self) {
        self.opaque.clear();
        self.alpha_cutout.clear();
        self.transparent.clear();
    }
}
