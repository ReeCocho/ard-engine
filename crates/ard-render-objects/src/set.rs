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
    /// Draw groups to render.
    groups: Vec<DrawGroup>,
    static_object_ranges: RenderableRanges,
    dynamic_object_ranges: RenderableRanges,
    static_group_ranges: RenderableRanges,
    dynamic_group_ranges: RenderableRanges,
}

pub struct RenderableSetUpdate<'a> {
    set: &'a mut RenderableSet,
    include_opaque: bool,
    include_alpha_cutout: bool,
    include_transparent: bool,
}

#[derive(Debug, Copy, Clone)]
pub struct DrawGroup {
    pub key: DrawKey,
    pub len: usize,
}

#[derive(Debug, Default, Clone)]
pub struct RenderableRanges {
    pub opaque: Range<usize>,
    pub alpha_cutout: Range<usize>,
    pub transparent: Range<usize>,
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
    pub fn static_group_ranges(&self) -> &RenderableRanges {
        &self.static_group_ranges
    }

    #[inline(always)]
    pub fn dynamic_group_ranges(&self) -> &RenderableRanges {
        &self.dynamic_group_ranges
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
            // the sorting function for transparent static objects is not consistent between frames
            // (it sorts by distance to camera).
            self.set.static_object_ranges.opaque = if self.include_opaque {
                Self::write_keyed_ids(
                    ids,
                    objects
                        .static_objects()
                        .opaque
                        .iter()
                        .filter(|&obj| filter_opaque(obj))
                        .map(|obj| (obj.key, obj.idx, 0.0)),
                    |id| bytemuck::cast::<_, DrawKey>(id.draw_idx),
                )
            } else {
                Range::default()
            };

            self.set.static_object_ranges.alpha_cutout = if self.include_alpha_cutout {
                Self::write_keyed_ids(
                    ids,
                    objects
                        .static_objects()
                        .alpha_cutout
                        .iter()
                        .filter(|&obj| filter_alpha_cut(obj))
                        .map(|obj| (obj.key, obj.idx, 0.0)),
                    |id| bytemuck::cast::<_, DrawKey>(id.draw_idx),
                )
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
        self.set.dynamic_object_ranges.opaque = if self.include_opaque {
            Self::write_keyed_ids(
                ids,
                objects
                    .dynamic_objects()
                    .opaque
                    .iter()
                    .filter(|&obj| filter_opaque(obj))
                    .map(|obj| (obj.key, obj.idx, 0.0)),
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
                    .iter()
                    .filter(|&obj| filter_alpha_cut(obj))
                    .map(|obj| (obj.key, obj.idx, 0.0)),
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
        if self.include_transparent {
            self.set.dynamic_object_ranges.transparent = Self::write_keyed_ids(
                ids,
                objects
                    .dynamic_objects()
                    .transparent
                    .iter()
                    .filter(|&obj| filter_transparent(obj))
                    .map(|obj| {
                        (
                            obj.key,
                            obj.idx,
                            // Fill the padding in with the distance from the view...
                            (view_location - obj.position).length_squared(),
                        )
                    }),
                // ... so we can sort by distance from the view
                |id| OrderedFloat::from(id._padding),
            );

            self.set.static_object_ranges.transparent = Self::write_keyed_ids(
                ids,
                objects
                    .static_objects()
                    .transparent
                    .iter()
                    .filter(|&obj| filter_transparent(obj))
                    .map(|obj| {
                        (
                            obj.key,
                            obj.idx,
                            (view_location - obj.position).length_squared(),
                        )
                    }),
                |id| OrderedFloat::from(id._padding),
            );

            self.set.dynamic_group_ranges.transparent = Self::compact_groups(
                &mut ids[self.set.dynamic_object_ranges.transparent.clone()],
                groups,
            );

            self.set.static_group_ranges.transparent = Self::compact_groups(
                &mut ids[self.set.static_object_ranges.transparent.clone()],
                groups,
            );
        } else {
            self.set.static_object_ranges.transparent = Range::default();
            self.set.dynamic_object_ranges.transparent = Range::default();
            self.set.static_group_ranges.transparent = Range::default();
            self.set.dynamic_group_ranges.transparent = Range::default();
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
