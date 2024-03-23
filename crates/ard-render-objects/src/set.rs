use std::ops::Range;

use ard_math::Vec3A;
use ard_render_base::resource::ResourceAllocator;
use ard_render_meshes::mesh::MeshResource;
use ordered_float::OrderedFloat;

use crate::{
    keys::DrawKey,
    objects::{AlphaCutoutObjectIndex, OpaqueObjectIndex, RenderObjects, TransparentObjectIndex},
};
use ard_render_si::types::GpuObjectId;

/// A set of objects and their draw calls to render objects.
///
/// # Note
/// The memory layout of object ids and draw calls are organized in a way that should be useful for
/// most renderers. The layout is as follows.
///
/// [Static | Opaque] [Static | Alpha Cutout] [Dynamic | Opaque] [Dynamic | Alpha Cutout] [Dynamic | Transparent] [Static | Transparent]
#[derive(Default)]
pub struct RenderableSet {
    /// Instances of objects to sort.
    object_instances: Vec<ObjectInstance>,
    /// Resulting object IDs for each object/meshlet pair.
    object_ids: Vec<GpuObjectId>,
    /// Draw groups to render.
    groups: Vec<DrawGroup>,
    static_object_ranges: RenderableRanges,
    dynamic_object_ranges: RenderableRanges,
    transparent_object_range: Range<usize>,
    static_group_ranges: RenderableRanges,
    dynamic_group_ranges: RenderableRanges,
    transparent_group_range: Range<usize>,
}

pub struct RenderableSetUpdate<'a> {
    set: &'a mut RenderableSet,
    include_opaque: bool,
    include_alpha_cutout: bool,
    include_transparent: bool,
}

/// A group represents a set of objects that have matching material and mesh, meaning they can all
/// be rendered with a single draw call.
#[derive(Debug, Copy, Clone)]
pub struct DrawGroup {
    pub key: DrawKey,
    pub len: usize,
}

#[derive(Debug, Default, Clone)]
pub struct RenderableRanges {
    pub opaque: Range<usize>,
    pub alpha_cutout: Range<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct ObjectInstance {
    key: DrawKey,
    id: u32,
    distance: OrderedFloat<f32>,
}

impl RenderableSet {
    #[inline(always)]
    pub fn ids(&self) -> &[GpuObjectId] {
        &self.object_ids
    }

    #[inline(always)]
    pub fn groups(&self) -> &[DrawGroup] {
        &self.groups
    }

    #[inline(always)]
    pub fn static_object_ranges(&self) -> &RenderableRanges {
        &self.static_object_ranges
    }

    #[inline(always)]
    pub fn dynamic_object_ranges(&self) -> &RenderableRanges {
        &self.dynamic_object_ranges
    }

    #[inline(always)]
    pub fn transparent_object_range(&self) -> &Range<usize> {
        &self.transparent_object_range
    }

    #[inline(always)]
    pub fn static_group_ranges(&self) -> &RenderableRanges {
        &self.static_group_ranges
    }

    #[inline(always)]
    pub fn dynamic_group_ranges(&self) -> &RenderableRanges {
        &self.dynamic_group_ranges
    }

    #[inline(always)]
    pub fn transparent_group_range(&self) -> &Range<usize> {
        &self.transparent_group_range
    }
}

impl<'a> RenderableSetUpdate<'a> {
    pub fn new(set: &'a mut RenderableSet) -> Self {
        Self {
            set,
            include_opaque: false,
            include_alpha_cutout: false,
            include_transparent: false,
        }
    }

    pub fn with_opaque(mut self) -> Self {
        self.include_opaque = true;
        self
    }

    pub fn with_alpha_cutout(mut self) -> Self {
        self.include_alpha_cutout = true;
        self
    }

    pub fn with_transparent(mut self) -> Self {
        self.include_transparent = true;
        self
    }

    pub fn update<const FIF: usize>(
        self,
        view_location: Vec3A,
        objects: &RenderObjects,
        meshes: &ResourceAllocator<MeshResource, FIF>,
        filter_opaque: impl Fn(&OpaqueObjectIndex) -> bool,
        filter_alpha_cut: impl Fn(&AlphaCutoutObjectIndex) -> bool,
        filter_transparent: impl Fn(&TransparentObjectIndex) -> bool,
    ) {
        let instances = &mut self.set.object_instances;
        let ids = &mut self.set.object_ids;
        let groups = &mut self.set.groups;

        // NOTE: The object ranges returned from calls to `write_keyed_instances` gives us back
        // ranges over the "instances", but what we really need are ranges over the "ids".
        instances.clear();

        // Either reset everything or ignore static objects and groups
        if objects.static_dirty() {
            ids.clear();
            groups.clear();

            // Write in opaque and alpha cut static objects and draws. These are the only kinds
            // of objects we can cache since dynamic objects have to be reset every frame and
            // the sorting function for transparent static objects is not consistent between
            // frames (it sorts by distance to camera).

            // Do opaque objects first
            let mut opaque_obj_count = 0;
            self.set.static_object_ranges.opaque = if self.include_opaque {
                objects.static_objects().values().for_each(|set| {
                    let base = set.block.base();
                    opaque_obj_count += Self::write_keyed_instances(
                        instances,
                        set.opaque
                            .indices
                            .iter()
                            .filter(|&obj| filter_opaque(obj))
                            .map(|obj| (obj.key, base + obj.idx, OrderedFloat::default())),
                        |id| id.key,
                    )
                    .len();
                });

                Range {
                    start: 0,
                    end: opaque_obj_count,
                }
            } else {
                Range::default()
            };

            // Then do alpha cutout
            let mut ac_obj_count = 0;
            self.set.static_object_ranges.alpha_cutout = if self.include_alpha_cutout {
                objects.static_objects().values().for_each(|set| {
                    let base = set.block.base();
                    ac_obj_count += Self::write_keyed_instances(
                        instances,
                        set.alpha_cutout
                            .indices
                            .iter()
                            .filter(|&obj| filter_alpha_cut(obj))
                            .map(|obj| (obj.key, base + obj.idx, OrderedFloat::default())),
                        |id| id.key,
                    )
                    .len();
                });

                Range {
                    start: opaque_obj_count,
                    end: opaque_obj_count + ac_obj_count,
                }
            } else {
                Range::default()
            };

            let start = ids.len();
            self.set.static_group_ranges.opaque = Self::compact_groups(
                &instances[self.set.static_object_ranges.opaque.clone()],
                ids,
                groups,
                meshes,
            );
            self.set.static_object_ranges.opaque = Range {
                start,
                end: ids.len(),
            };

            let start = ids.len();
            self.set.static_group_ranges.alpha_cutout = Self::compact_groups(
                &mut instances[self.set.static_object_ranges.alpha_cutout.clone()],
                ids,
                groups,
                meshes,
            );
            self.set.static_object_ranges.alpha_cutout = Range {
                start,
                end: ids.len(),
            };
        } else {
            ids.truncate(
                self.set.static_object_ranges.opaque.len()
                    + self.set.static_object_ranges.alpha_cutout.len(),
            );
            groups.truncate(
                self.set.static_group_ranges.opaque.len()
                    + self.set.static_group_ranges.alpha_cutout.len(),
            );
        }

        // Add in dynamic opaque and dynamic alpha cutout objects
        let base = objects.dynamic_objects().block.base();

        self.set.dynamic_object_ranges.opaque = if self.include_opaque {
            Self::write_keyed_instances(
                instances,
                objects
                    .dynamic_objects()
                    .opaque
                    .indices
                    .iter()
                    .filter(|&obj| filter_opaque(obj))
                    .map(|obj| (obj.key, base + obj.idx, OrderedFloat::default())),
                |id| id.key,
            )
        } else {
            Range::default()
        };

        self.set.dynamic_object_ranges.alpha_cutout = if self.include_alpha_cutout {
            Self::write_keyed_instances(
                instances,
                objects
                    .dynamic_objects()
                    .alpha_cutout
                    .indices
                    .iter()
                    .filter(|&obj| filter_alpha_cut(obj))
                    .map(|obj| (obj.key, base + obj.idx, OrderedFloat::default())),
                |id| id.key,
            )
        } else {
            Range::default()
        };

        let start = ids.len();
        self.set.dynamic_group_ranges.opaque = Self::compact_groups(
            &mut instances[self.set.dynamic_object_ranges.opaque.clone()],
            ids,
            groups,
            meshes,
        );
        self.set.dynamic_object_ranges.opaque = Range {
            start,
            end: ids.len(),
        };

        let start = ids.len();
        self.set.dynamic_group_ranges.alpha_cutout = Self::compact_groups(
            &mut instances[self.set.dynamic_object_ranges.alpha_cutout.clone()],
            ids,
            groups,
            meshes,
        );
        self.set.dynamic_object_ranges.alpha_cutout = Range {
            start,
            end: ids.len(),
        };

        // Add in dynamic and static transparent objects
        if self.include_transparent {
            let dyn_objects = objects
                .dynamic_objects()
                .transparent
                .indices
                .iter()
                .zip(Some(base).into_iter().cycle());

            let static_objects = objects.static_objects().values().flat_map(|set| {
                set.transparent
                    .indices
                    .iter()
                    .zip(Some(set.block.base()).into_iter().cycle())
            });

            self.set.transparent_object_range = Self::write_keyed_instances(
                instances,
                dyn_objects
                    .chain(static_objects)
                    .filter(|(obj, _)| filter_transparent(obj))
                    .map(|(obj, base)| {
                        (
                            obj.key,
                            base + obj.idx,
                            // Fill the padding in with the distance from the view...
                            (-(view_location - obj.position).length_squared()).into(),
                        )
                    }),
                |id| id.distance,
            );

            let start = ids.len();
            self.set.transparent_group_range = Self::compact_groups(
                &mut instances[self.set.transparent_object_range.clone()],
                ids,
                groups,
                meshes,
            );
            self.set.transparent_object_range = Range {
                start,
                end: ids.len(),
            };
        } else {
            self.set.transparent_object_range = Range::default();
            self.set.transparent_group_range = Range::default();
        }
    }

    fn write_keyed_instances<K: Ord>(
        buff: &mut Vec<ObjectInstance>,
        src: impl Iterator<Item = (DrawKey, u32, OrderedFloat<f32>)>,
        sort: impl FnMut(&ObjectInstance) -> K,
    ) -> Range<usize> {
        let start = buff.len();

        for (key, idx, distance) in src {
            buff.push(ObjectInstance {
                key,
                id: idx,
                distance,
            });
        }

        let rng = Range {
            start,
            end: buff.len(),
        };

        buff[rng.clone()].sort_unstable_by_key(sort);

        rng
    }

    fn compact_groups<const FIF: usize>(
        instances: &[ObjectInstance],
        ids: &mut Vec<GpuObjectId>,
        groups: &mut Vec<DrawGroup>,
        meshes: &ResourceAllocator<MeshResource, FIF>,
    ) -> Range<usize> {
        let start = groups.len();

        if instances.is_empty() {
            return Range { start, end: start };
        }

        let mut cur_key = instances[0].key;
        let mut cur_mesh = meshes.get(cur_key.separate().mesh_id).unwrap();
        groups.push(DrawGroup {
            key: cur_key,
            len: 0,
        });

        instances.iter().for_each(|instance| {
            // Create a new group if we encounter a new key
            let new_key = instance.key;
            if new_key != cur_key {
                cur_key = new_key;
                cur_mesh = meshes.get(cur_key.separate().mesh_id).unwrap();
                groups.push(DrawGroup {
                    key: cur_key,
                    len: 0,
                });
            }

            // Update the draw count
            let draw_idx = groups.len() - 1;
            groups[draw_idx].len += cur_mesh.meshlet_count;

            // Create IDs
            for i in 0..(cur_mesh.meshlet_count) {
                ids.push(GpuObjectId {
                    // The highest bit of data idx is reserved as a visibility flag for objects.
                    // New objects are always considered visible.
                    data_idx: instance.id | (1 << 31),
                    meshlet: cur_mesh.block.meshlet_block().base() + i as u32,
                });
            }
        });

        Range {
            start,
            end: groups.len(),
        }
    }
}
