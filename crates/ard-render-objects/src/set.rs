use std::ops::Range;

use ard_math::Vec3A;
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
    /// Object IDs for sorting draw calls or GPU driven rendering.
    object_ids: Vec<GpuObjectId>,
    non_transparent_object_count: usize,
    /// Draw groups to render.
    groups: Vec<DrawGroup>,
    non_transparent_draw_count: usize,
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

impl RenderableSet {
    #[inline(always)]
    pub fn ids(&self) -> &[GpuObjectId] {
        &self.object_ids
    }

    #[inline(always)]
    pub fn non_transparent_object_count(&self) -> usize {
        self.non_transparent_object_count
    }

    #[inline(always)]
    pub fn non_transparent_draw_count(&self) -> usize {
        self.non_transparent_draw_count
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

    pub fn update(
        self,
        view_location: Vec3A,
        objects: &RenderObjects,
        filter_opaque: impl Fn(&OpaqueObjectIndex) -> bool,
        filter_alpha_cut: impl Fn(&AlphaCutoutObjectIndex) -> bool,
        filter_transparent: impl Fn(&TransparentObjectIndex) -> bool,
    ) {
        let ids = &mut self.set.object_ids;
        let groups = &mut self.set.groups;

        // NOTE: Instead of writing the batch index here, we write the draw key so that we can
        // sort the object IDs here in place and then later determine the batch index. This is
        // fine as long as the size of object used for `batch_idx` is the same as the size used
        // for `DrawKey`.

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
                    opaque_obj_count += Self::write_keyed_ids(
                        ids,
                        set.opaque
                            .indices
                            .iter()
                            .filter(|&obj| filter_opaque(obj))
                            .map(|obj| (obj.key, base + obj.idx, 0.0)),
                        |id| bytemuck::cast::<_, DrawKey>(id.draw_idx),
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
                    ac_obj_count += Self::write_keyed_ids(
                        ids,
                        set.alpha_cutout
                            .indices
                            .iter()
                            .filter(|&obj| filter_alpha_cut(obj))
                            .map(|obj| (obj.key, base + obj.idx, 0.0)),
                        |id| bytemuck::cast::<_, DrawKey>(id.draw_idx),
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

            self.set.static_group_ranges.opaque = Self::compact_groups(
                &mut ids[self.set.static_object_ranges.opaque.clone()],
                groups,
            );

            self.set.static_group_ranges.alpha_cutout = Self::compact_groups(
                &mut ids[self.set.static_object_ranges.alpha_cutout.clone()],
                groups,
            );
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
            Self::write_keyed_ids(
                ids,
                objects
                    .dynamic_objects()
                    .opaque
                    .indices
                    .iter()
                    .filter(|&obj| filter_opaque(obj))
                    .map(|obj| (obj.key, base + obj.idx, 0.0)),
                |id| bytemuck::cast::<_, DrawKey>(id.draw_idx),
            )
        } else {
            Range::default()
        };

        self.set.dynamic_object_ranges.alpha_cutout = if self.include_alpha_cutout {
            Self::write_keyed_ids(
                ids,
                objects
                    .dynamic_objects()
                    .alpha_cutout
                    .indices
                    .iter()
                    .filter(|&obj| filter_alpha_cut(obj))
                    .map(|obj| (obj.key, base + obj.idx, 0.0)),
                |id| bytemuck::cast::<_, DrawKey>(id.draw_idx),
            )
        } else {
            Range::default()
        };

        self.set.dynamic_group_ranges.opaque = Self::compact_groups(
            &mut ids[self.set.dynamic_object_ranges.opaque.clone()],
            groups,
        );

        self.set.dynamic_group_ranges.alpha_cutout = Self::compact_groups(
            &mut ids[self.set.dynamic_object_ranges.alpha_cutout.clone()],
            groups,
        );

        // Add in dynamic and static transparent objects
        self.set.non_transparent_object_count = ids.len();
        self.set.non_transparent_draw_count = groups.len();

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

            self.set.transparent_object_range = Self::write_keyed_ids(
                ids,
                dyn_objects
                    .chain(static_objects)
                    .filter(|(obj, _)| filter_transparent(obj))
                    .map(|(obj, base)| {
                        (
                            obj.key,
                            base + obj.idx,
                            // Fill the padding in with the distance from the view...
                            -(view_location - obj.position).length_squared(),
                        )
                    }),
                |id| OrderedFloat::from(id._padding),
            );

            self.set.transparent_group_range =
                Self::compact_groups(&mut ids[self.set.transparent_object_range.clone()], groups);
        } else {
            self.set.transparent_object_range = Range::default();
            self.set.transparent_group_range = Range::default();
        }
    }

    fn write_keyed_ids<K: Ord>(
        buff: &mut Vec<GpuObjectId>,
        src: impl Iterator<Item = (DrawKey, u32, f32)>,
        sort: impl FnMut(&GpuObjectId) -> K,
    ) -> Range<usize> {
        let start = buff.len();

        for (key, idx, padding) in src {
            buff.push(GpuObjectId {
                draw_idx: bytemuck::cast(key),
                data_idx: idx,
                _padding: padding,
            });
        }

        let rng = Range {
            start,
            end: buff.len(),
        };

        buff[rng.clone()].sort_unstable_by_key(sort);

        rng
    }

    fn compact_groups(ids: &mut [GpuObjectId], groups: &mut Vec<DrawGroup>) -> Range<usize> {
        let start = groups.len();

        if ids.is_empty() {
            return Range { start, end: start };
        }

        let mut cur_key: DrawKey = bytemuck::cast(ids[0].draw_idx);
        groups.push(DrawGroup {
            key: cur_key,
            len: 0,
        });

        ids.iter_mut().for_each(|id| {
            // Create a new group if we encounter a new key
            let new_key = bytemuck::cast(id.draw_idx);
            if new_key != cur_key {
                cur_key = new_key;
                groups.push(DrawGroup {
                    key: cur_key,
                    len: 0,
                });
            }

            // Update the draw count
            let draw_idx = groups.len() - 1;
            groups[draw_idx].len += 1;
            id.draw_idx[0] = draw_idx as u32;
            id.draw_idx[1] = draw_idx as u32;
        });

        Range {
            start,
            end: groups.len(),
        }
    }
}
