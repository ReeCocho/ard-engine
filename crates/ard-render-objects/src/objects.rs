use ard_alloc::buddy::{BuddyAllocator, BuddyBlock};
use ard_core::{
    prelude::{Disabled, Static},
    stat::{DirtyStaticListener, StaticGroup},
};
use ard_ecs::prelude::Entity;
use ard_log::info;
use ard_math::Vec3A;
use ard_pal::prelude::{
    Buffer, BufferCreateInfo, BufferUsage, BufferWriteView, Context, MemoryUsage, QueueTypes,
    SharingMode,
};
use ard_render_material::material_instance::MaterialInstance;
use ard_render_meshes::mesh::Mesh;
use fxhash::FxHashMap;

use crate::{keys::DrawKey, Model, RenderFlags, RenderingMode};
use ard_render_si::types::GpuObjectData;

const BASE_BLOCK_COUNT: usize = 64;
const DEFAULT_OBJECT_DATA_COUNT: usize = 1024;

/// Contains a complete collection of objects to render.
pub struct RenderObjects {
    /// The actual per-instance object data.
    object_data: Buffer,
    /// The virtual allocator used by `object_data`.
    alloc: BuddyAllocator,
    /// Maps static groups to their associated sets.
    static_objects: FxHashMap<StaticObjectGroup, ObjectSet>,
    /// Set for all dynamic objects.
    dynamic_objects: ObjectSet,
    /// Flag indicating at least one static group was marked dirty.
    any_dirty_static: bool,
    /// Groups which were marked as dirty.
    dirty_static_groups: FxHashMap<StaticObjectGroup, bool>,
}

/// An object set represents a logical group of objects, with a list for each rendering mode.
pub struct ObjectSet {
    pub data: Vec<GpuObjectData>,
    pub block: BuddyBlock,
    pub opaque: ObjectList<OpaqueObjectIndex>,
    pub alpha_cutout: ObjectList<AlphaCutoutObjectIndex>,
    pub transparent: ObjectList<TransparentObjectIndex>,
}

/// An object list is a sequence of object indices.
pub struct ObjectList<T> {
    pub indices: Vec<T>,
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StaticObjectGroup {
    Group(StaticGroup),
    Ungrouped,
}

impl RenderObjects {
    pub fn new(ctx: Context) -> Self {
        let mut alloc = BuddyAllocator::new(BASE_BLOCK_COUNT, DEFAULT_OBJECT_DATA_COUNT);

        Self {
            object_data: Buffer::new(
                ctx,
                BufferCreateInfo {
                    size: (DEFAULT_OBJECT_DATA_COUNT
                        * BASE_BLOCK_COUNT
                        * std::mem::size_of::<GpuObjectData>()) as u64,
                    array_elements: 1,
                    buffer_usage: BufferUsage::STORAGE_BUFFER,
                    memory_usage: MemoryUsage::CpuToGpu,
                    queue_types: QueueTypes::MAIN | QueueTypes::COMPUTE,
                    sharing_mode: SharingMode::Exclusive,
                    debug_name: Some("object_data".into()),
                },
            )
            .unwrap(),
            static_objects: {
                let mut groups = FxHashMap::default();
                groups.insert(
                    StaticObjectGroup::Ungrouped,
                    ObjectSet::new(alloc.allocate(1).unwrap()),
                );
                groups.insert(
                    StaticObjectGroup::Group(0),
                    ObjectSet::new(alloc.allocate(1).unwrap()),
                );
                groups.insert(
                    StaticObjectGroup::Group(1),
                    ObjectSet::new(alloc.allocate(1).unwrap()),
                );
                groups
            },
            dynamic_objects: ObjectSet::new(alloc.allocate(1).unwrap()),
            alloc,
            dirty_static_groups: {
                let mut groups = FxHashMap::default();
                groups.insert(StaticObjectGroup::Ungrouped, true);
                groups.insert(StaticObjectGroup::Group(0), true);
                groups.insert(StaticObjectGroup::Group(1), true);
                groups
            },
            any_dirty_static: true,
        }
    }

    #[inline(always)]
    pub fn object_data(&self) -> &Buffer {
        &self.object_data
    }

    #[inline(always)]
    pub fn static_dirty(&self) -> bool {
        self.any_dirty_static
    }

    pub fn upload_objects<'a>(
        &mut self,
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
        static_dirty: &DirtyStaticListener,
    ) {
        // Update dirty flags
        self.any_dirty_static = false;
        self.dirty_static_groups
            .values_mut()
            .for_each(|v| *v = false);
        while let Some(group) = static_dirty.recv() {
            self.any_dirty_static = true;

            match self
                .dirty_static_groups
                .get_mut(&StaticObjectGroup::Group(group))
            {
                Some(group_dirty) => *group_dirty = true,
                None => {
                    *self
                        .dirty_static_groups
                        .get_mut(&StaticObjectGroup::Ungrouped)
                        .unwrap() = true
                }
            }
        }

        // Write in every object into the buffer
        let mut view = self.object_data.write(0).unwrap();

        // TODO: Handle grouped static geometry

        // Ungrouped static geometry
        if self.any_dirty_static {
            // Reset all dirty groups
            self.dirty_static_groups
                .iter()
                .filter(|(_, is_dirty)| **is_dirty)
                .for_each(|(group, _)| {
                    self.static_objects.get_mut(group).unwrap().clear();
                });

            // Write in renderable objects for dirty groups
            for (e, (mesh, mat, mdl, mode, flags, group), disabled) in static_objs {
                // Ignore if disbaled
                if disabled.is_some() {
                    continue;
                }

                let group = match self
                    .dirty_static_groups
                    .get(&StaticObjectGroup::Group(group.0))
                {
                    Some(_) => StaticObjectGroup::Group(group.0),
                    None => StaticObjectGroup::Ungrouped,
                };

                // Ignore if not a dirty group
                if !*self.dirty_static_groups.get(&group).unwrap() {
                    continue;
                }

                Self::write_renderable(
                    e,
                    (mesh, mat, mdl, mode, flags),
                    self.static_objects.get_mut(&group).unwrap(),
                );
            }

            // Flush dirty groups
            self.dirty_static_groups
                .iter()
                .filter(|(_, is_dirty)| **is_dirty)
                .for_each(|(group, _)| {
                    self.static_objects
                        .get_mut(group)
                        .unwrap()
                        .flush_to_buffer(&mut view, &mut self.alloc);
                });
        }

        // Write in dynamic geometry
        self.dynamic_objects.clear();
        for (e, (mesh, mat, mdl, mode, flags), disabled) in dynamic_objs {
            if disabled.is_some() {
                continue;
            }

            Self::write_renderable(e, (mesh, mat, mdl, mode, flags), &mut self.dynamic_objects);
        }

        self.dynamic_objects
            .flush_to_buffer(&mut view, &mut self.alloc);
    }

    #[inline(always)]
    pub fn static_objects(&self) -> &FxHashMap<StaticObjectGroup, ObjectSet> {
        &self.static_objects
    }

    #[inline(always)]
    pub fn dynamic_objects(&self) -> &ObjectSet {
        &self.dynamic_objects
    }

    #[inline]
    fn write_renderable(
        entity: Entity,
        query: (
            &Mesh,
            &MaterialInstance,
            &Model,
            &RenderingMode,
            &RenderFlags,
        ),
        set: &mut ObjectSet,
    ) {
        let (mesh, mat, mdl, mode, flags) = query;

        // Write the object ID and data to the appropriate list based on the rendering mode
        let data = GpuObjectData {
            model: mdl.0,
            normal: mdl.0.inverse().transpose(),
            entity_id: entity.id(),
            entity_ver: entity.ver(),
            material: mat.data_slot().map(|slot| slot.into()).unwrap_or_default(),
            textures: mat.tex_slot().map(|slot| slot.into()).unwrap_or_default(),
        };

        match *mode {
            RenderingMode::Opaque => {
                set.push_opaque(data, DrawKey::new(mat, mesh), *flags);
            }
            RenderingMode::AlphaCutout => {
                set.push_alpha_cutout(data, DrawKey::new(mat, mesh), *flags);
            }
            RenderingMode::Transparent => {
                set.push_transparent(data, DrawKey::new(mat, mesh), *flags, mdl.position());
            }
        };
    }
}

impl<T> Default for ObjectList<T> {
    fn default() -> Self {
        Self {
            indices: Vec::default(),
        }
    }
}

impl<T> ObjectList<T> {
    #[inline]
    pub fn clear(&mut self) {
        self.indices.clear();
    }

    #[inline]
    pub fn push(&mut self, idx: T) {
        self.indices.push(idx);
    }
}

impl ObjectSet {
    pub fn new(block: BuddyBlock) -> Self {
        Self {
            block,
            data: Vec::default(),
            opaque: ObjectList::default(),
            alpha_cutout: ObjectList::default(),
            transparent: ObjectList::default(),
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        self.data.clear();
        self.opaque.clear();
        self.alpha_cutout.clear();
        self.transparent.clear();
    }

    #[inline]
    pub fn push_opaque(&mut self, data: GpuObjectData, key: DrawKey, flags: RenderFlags) {
        self.opaque.indices.push(OpaqueObjectIndex {
            key,
            flags,
            idx: self.data.len() as u32,
        });
        self.data.push(data);
    }

    #[inline]
    pub fn push_alpha_cutout(&mut self, data: GpuObjectData, key: DrawKey, flags: RenderFlags) {
        self.alpha_cutout.indices.push(AlphaCutoutObjectIndex {
            key,
            flags,
            idx: self.data.len() as u32,
        });
        self.data.push(data);
    }

    #[inline]
    pub fn push_transparent(
        &mut self,
        data: GpuObjectData,
        key: DrawKey,
        flags: RenderFlags,
        position: Vec3A,
    ) {
        self.transparent.indices.push(TransparentObjectIndex {
            key,
            flags,
            position,
            idx: self.data.len() as u32,
        });
        self.data.push(data);
    }

    pub fn flush_to_buffer(&mut self, view: &mut BufferWriteView, alloc: &mut BuddyAllocator) {
        // First, we must ensure the allocation has enough capacity for our objects
        if (self.block.len() as usize) < self.data.len() {
            // Free the old block
            alloc.free(self.block);

            // Allocation expansion required
            info!("Expanding object data block.");
            alloc.reserve_for(self.data.len());
            view.expand(
                (alloc.block_count() * std::mem::size_of::<GpuObjectData>()) as u64,
                true,
            )
            .unwrap();

            // Allocate the new block. Guaranteed to succeed now since we reserved space
            self.block = alloc.allocate(self.data.len()).unwrap();
        }

        // Write in all object data into the view
        self.data
            .iter()
            .enumerate()
            .for_each(|(i, data)| view.set_as_array(*data, self.block.base() as usize + i));
    }
}
