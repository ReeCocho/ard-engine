use ard_formats::mesh::VertexLayout;
use ard_render_base::resource::ResourceId;
use ard_render_material::material_instance::MaterialInstance;
use ard_render_meshes::mesh::Mesh;
use bytemuck::{Pod, Zeroable};

/// A draw key is used to sort objects for optimal performance when rendering.
///
/// The layout of a draw key is as follows.
///
/// [Material][Vertex Layout][Mesh   ][MaterialInstance]
/// [ 24 bits][       8 bits][16 bits][         16 bits]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DrawKey(u64);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SeparatedDrawKey {
    pub material_id: ResourceId,
    pub vertex_layout: VertexLayout,
    pub mesh_id: ResourceId,
    pub material_instance_id: ResourceId,
}

impl DrawKey {
    #[inline(always)]
    pub fn new(material_instance: &MaterialInstance, mesh: &Mesh) -> Self {
        let mut out = 0u64;
        out |= (u64::from(material_instance.material().id()) & ((1 << 24) - 1)) << 40;
        out |= (mesh.layout().bits() as u64 & ((1 << 8) - 1)) << 32;
        out |= (u64::from(mesh.id()) & ((1 << 16) - 1)) << 16;
        out |= u64::from(material_instance.id()) & ((1 << 16) - 1);
        Self(out)
    }

    #[inline(always)]
    pub fn separate(&self) -> SeparatedDrawKey {
        SeparatedDrawKey {
            material_id: ResourceId::from((self.0 >> 40) as usize & ((1 << 24) - 1)),
            vertex_layout: VertexLayout::from_bits_truncate(
                ((self.0 >> 32) & ((1 << 8) - 1)) as u8,
            ),
            mesh_id: ResourceId::from((self.0 >> 16) as usize & ((1 << 16) - 1)),
            material_instance_id: ResourceId::from(self.0 as usize & ((1 << 16) - 1)),
        }
    }
}

unsafe impl Pod for DrawKey {}
unsafe impl Zeroable for DrawKey {}

impl From<u64> for DrawKey {
    fn from(value: u64) -> Self {
        DrawKey(value)
    }
}

impl From<DrawKey> for u64 {
    fn from(value: DrawKey) -> Self {
        value.0
    }
}
