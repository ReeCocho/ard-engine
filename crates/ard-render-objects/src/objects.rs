use ard_alloc::buddy::{BuddyAllocator, BuddyBlock};
use ard_core::{
    prelude::{Disabled, Static},
    stat::{DirtyStaticListener, StaticGroup},
};
use ard_ecs::prelude::Entity;
use ard_log::info;
use ard_math::{Vec4, Vec4Swizzles};
use ard_pal::prelude::{
    Buffer, BufferCreateInfo, BufferUsage, BufferWriteView, Context, MemoryUsage, QueueTypes,
    SharingMode,
};
use ard_render_base::{
    ecs::Frame,
    resource::{ResourceAllocator, ResourceId},
    RenderingMode,
};
use ard_render_material::{material::MaterialResource, material_instance::MaterialInstance};
use ard_render_meshes::mesh::{Mesh, MeshResource};
use rustc_hash::FxHashMap;

use crate::{keys::DrawKey, Model, RenderFlags};
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
    /// Counter for determining if `object_data` has expanded.
    buffer_expanded: u32,
}

/// An object set represents a logical group of objects, with a list for each rendering mode.
pub struct ObjectSet {
    pub data: Vec<GpuObjectData>,
    pub missing_blas: Vec<usize>,
    pub block: BuddyBlock,
    pub opaque: ObjectList<ObjectIndex>,
    pub alpha_cutout: ObjectList<ObjectIndex>,
    pub transparent: ObjectList<ObjectIndex>,
}

/// An object list is a sequence of object indices.
pub struct ObjectList<T> {
    pub indices: Vec<T>,
}

pub struct ObjectIndex {
    pub key: DrawKey,
    pub flags: RenderFlags,
    pub idx: u32,
    pub bounding_sphere: Vec4,
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
                    buffer_usage: BufferUsage::STORAGE_BUFFER | BufferUsage::DEVICE_ADDRESS,
                    memory_usage: MemoryUsage::CpuToGpu,
                    queue_types: QueueTypes::MAIN | QueueTypes::COMPUTE,
                    sharing_mode: SharingMode::Concurrent,
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
            buffer_expanded: 2,
        }
    }

    #[inline(always)]
    pub fn buffer_expanded(&self) -> bool {
        self.buffer_expanded > 0
    }

    #[inline(always)]
    pub fn object_data(&self) -> &Buffer {
        &self.object_data
    }

    #[inline(always)]
    pub fn static_dirty(&self) -> bool {
        self.any_dirty_static
    }

    // Takes objects from the primary ECS and converts them to the format used by the renderer.
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
        static_dirty: &DirtyStaticListener,
    ) {
        // Update dirty flags
        self.buffer_expanded = self.buffer_expanded.saturating_sub(1);
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
                    frame,
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
                    self.static_objects.get_mut(group).unwrap().flush_to_buffer(
                        &mut view,
                        &mut self.alloc,
                        &mut self.buffer_expanded,
                    );
                });
        }

        // Write in dynamic geometry
        self.dynamic_objects.clear();
        for (e, (mesh, mat, mdl, mode, flags), disabled) in dynamic_objs {
            if disabled.is_some() {
                continue;
            }

            Self::write_renderable(
                frame,
                e,
                (mesh, mat, mdl, mode, flags),
                &mut self.dynamic_objects,
            );
        }

        self.dynamic_objects
            .flush_to_buffer(&mut view, &mut self.alloc, &mut self.buffer_expanded);
    }

    // Looks to see if any objects that were flagged as missing a BLAS can have the BLAS set.
    pub fn check_for_blas(&mut self, meshes: &ResourceAllocator<MeshResource>) {
        let mut view = self.object_data.write(0).unwrap();
        self.dynamic_objects.check_for_blas(&mut view, meshes);
        self.static_objects
            .values_mut()
            .for_each(|set| set.check_for_blas(&mut view, meshes));
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
        frame: Frame,
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
        let mdl_inv = mdl.0.inverse();

        // Transform the bounding sphere to be in world space
        let mut bounding_sphere = mesh.bounding_sphere();
        let new_center = mdl.0 * Vec4::from((bounding_sphere.xyz(), 1.0));
        let new_radius = bounding_sphere.w * mdl.scale().max_element();
        bounding_sphere = Vec4::from((new_center.xyz(), new_radius));

        // Lookup BLAS (might not be ready yet, but 0 values are allowed by the spec).
        let blas = mesh.blas();

        // SBT offsets conversion
        let sbt = (usize::from(mat.material().id()) * MaterialResource::RT_GROUPS_PER_MATERIAL)
            + MaterialResource::to_group_idx(*mode, mesh.layout());

        // Write the object ID and data to the appropriate list based on the rendering mode
        let data = GpuObjectData {
            // VkAccelerationStructureInstanceKHR
            model: [mdl.0.row(0), mdl.0.row(1), mdl.0.row(2)],
            // The object ID is written to the instance field later.
            instance_mask: (0xFF << 24),
            // SBT Offset and geometry flags.
            // NOTE: The flags come from `VkGeometryInstanceFlagBitsKHR`
            shader_flags: (sbt as u32 & 0xFFFFFF) | (1 << 24),
            blas,
            // Our stuff
            model_inv: [mdl_inv.row(0), mdl_inv.row(1), mdl_inv.row(2)],
            mesh: usize::from(mesh.id()) as u16,
            textures: mat
                .tex_slot()
                .map(|slot| u16::from(slot))
                .unwrap_or_default(),
            entity: u32::from(entity),
            material: mat
                .data_ptrs()
                .map(|ptrs| ptrs[usize::from(frame)])
                .unwrap_or_default(),
        };

        // If the mesh being used is missing it's final BLAS, we mark the entity for a later update
        // when the BLAS is ready.
        if blas == 0 {
            set.missing_blas.push(set.data.len());
        }

        match *mode {
            RenderingMode::Opaque => {
                set.push_opaque(data, DrawKey::new(mat, mesh), *flags, bounding_sphere);
            }
            RenderingMode::AlphaCutout => {
                set.push_alpha_cutout(data, DrawKey::new(mat, mesh), *flags, bounding_sphere);
            }
            RenderingMode::Transparent => {
                set.push_transparent(data, DrawKey::new(mat, mesh), *flags, bounding_sphere);
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
            missing_blas: Vec::default(),
            opaque: ObjectList::default(),
            alpha_cutout: ObjectList::default(),
            transparent: ObjectList::default(),
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        self.data.clear();
        self.missing_blas.clear();
        self.opaque.clear();
        self.alpha_cutout.clear();
        self.transparent.clear();
    }

    #[inline]
    pub fn push_opaque(
        &mut self,
        data: GpuObjectData,
        key: DrawKey,
        flags: RenderFlags,
        bounding_sphere: Vec4,
    ) {
        self.opaque.indices.push(ObjectIndex {
            key,
            flags,
            bounding_sphere,
            idx: self.data.len() as u32,
        });
        self.data.push(data);
    }

    #[inline]
    pub fn push_alpha_cutout(
        &mut self,
        data: GpuObjectData,
        key: DrawKey,
        flags: RenderFlags,
        bounding_sphere: Vec4,
    ) {
        self.alpha_cutout.indices.push(ObjectIndex {
            key,
            flags,
            bounding_sphere,
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
        bounding_sphere: Vec4,
    ) {
        self.transparent.indices.push(ObjectIndex {
            key,
            flags,
            bounding_sphere,
            idx: self.data.len() as u32,
        });
        self.data.push(data);
    }

    pub fn flush_to_buffer(
        &mut self,
        view: &mut BufferWriteView,
        alloc: &mut BuddyAllocator,
        buffer_expanded: &mut u32,
    ) {
        // First, we must ensure the allocation has enough capacity for our objects
        if (self.block.len() as usize) < self.data.len() {
            // Free the old block
            alloc.free(self.block);

            // Allocation expansion required
            info!("Expanding object data block.");
            alloc.reserve_for(self.data.len());
            if view
                .expand(
                    (alloc.block_count() * std::mem::size_of::<GpuObjectData>()) as u64,
                    true,
                )
                .unwrap()
            {
                *buffer_expanded = 1;
            }

            // Allocate the new block. Guaranteed to succeed now since we reserved space
            self.block = alloc.allocate(self.data.len()).unwrap();
        }

        // Write in all object data into the view
        self.data.iter_mut().enumerate().for_each(|(i, data)| {
            let object_id = self.block.base() + i as u32;
            data.instance_mask |= object_id & 0xFFFFFF;
            view.set_as_array(*data, object_id as usize);
        });
    }

    pub fn check_for_blas(
        &mut self,
        view: &mut BufferWriteView,
        meshes: &ResourceAllocator<MeshResource>,
    ) {
        if self.missing_blas.is_empty() {
            return;
        }

        // Loop backwards over the list and swap elements from the rear into spots that
        // are removed (this is fine since the ordering doesn't matter).
        let mut i = self.missing_blas.len() - 1;
        loop {
            // Get the object and the mesh we are checking
            let idx = self.missing_blas[i];
            let data = &mut self.data[idx];
            let mesh = meshes.get(ResourceId::from(data.mesh as usize)).unwrap();

            // If the BLAS is ready now, write it in and remove this element from the pending list
            if mesh.blas_ready {
                data.blas = mesh.blas.device_ref();
                view.set_as_array(*data, (data.instance_mask & 0xFFFF) as usize);
                self.missing_blas.swap_remove(i);
            }

            if i == 0 {
                break;
            } else {
                i -= 1;
            }
        }
    }
}
