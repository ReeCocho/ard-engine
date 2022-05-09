use std::{collections::hash_map::DefaultHasher, hash::Hasher};

use renderer::forward_plus::DrawKey;

use crate::prelude::*;

#[derive(Default)]
pub struct FastIntHasher {
    hash: u64,
}

impl Hasher for FastIntHasher {
    #[inline]
    fn write_u32(&mut self, n: u32) {
        debug_assert_eq!(self.hash, 0);
        self.hash = n as u64;
    }

    #[inline]
    fn write_u64(&mut self, n: u64) {
        debug_assert_eq!(self.hash, 0);
        self.hash = n;
    }

    #[inline]
    fn write_u128(&mut self, n: u128) {
        debug_assert_eq!(self.hash, 0);
        self.hash = n as u64;
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        debug_assert_eq!(self.hash, 0);
        let mut hasher = DefaultHasher::default();
        hasher.write(bytes);
        self.hash = hasher.finish();
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

#[inline]
pub(crate) fn make_layout_key(layout: &VertexLayout) -> u8 {
    let mut key = 0;
    if layout.normals {
        key |= 1 << 0;
    }

    if layout.tangents {
        key |= 1 << 1;
    }

    if layout.colors {
        key |= 1 << 2;
    }

    if layout.uv0 {
        key |= 1 << 3;
    }

    if layout.uv1 {
        key |= 1 << 3;
    }

    if layout.uv2 {
        key |= 1 << 3;
    }

    if layout.uv3 {
        key |= 1 << 3;
    }

    key
}

#[inline]
pub(crate) fn from_layout_key(key: u8) -> VertexLayout {
    let mut layout = VertexLayout::default();

    if key & (1 << 0) != 0 {
        layout.normals = true;
    }

    if key & (1 << 1) != 0 {
        layout.tangents = true;
    }

    if key & (1 << 2) != 0 {
        layout.colors = true;
    }

    if key & (1 << 3) != 0 {
        layout.uv0 = true;
    }

    if key & (1 << 4) != 0 {
        layout.uv1 = true;
    }

    if key & (1 << 5) != 0 {
        layout.uv2 = true;
    }

    if key & (1 << 6) != 0 {
        layout.uv3 = true;
    }

    layout
}

#[inline]
pub(crate) fn make_draw_key(material: &Material, mesh: &Mesh) -> DrawKey {
    // [Pipeline][Vertex Layout][Mesh   ][Material]
    // [ 25 bits][       7 bits][16 bits][16 bits]

    // Upper 10 bits are pipeline. Middle 11 are material. Bottom 11 are mesh.
    let mut out = 0;
    out |= (material.pipeline_id as u64 & ((1 << 25) - 1)) << 39;
    out |= (mesh.layout_key as u64 & ((1 << 7) - 1)) << 32;
    out |= (mesh.id as u64 & ((1 << 16) - 1)) << 16;
    out |= material.id as u64 & ((1 << 16) - 1);
    out
}

/// Pipeline, vertex layout key, mesh, material
#[inline]
pub(crate) fn from_draw_key(key: DrawKey) -> (u32, VertexLayoutKey, u32, u32) {
    (
        (key >> 39) as u32 & ((1 << 25) - 1),
        (key >> 32) as VertexLayoutKey & ((1 << 7) - 1),
        (key >> 16) as u32 & ((1 << 16) - 1),
        key as u32 & ((1 << 16) - 1),
    )
}
