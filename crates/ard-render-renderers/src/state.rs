use ard_formats::mesh::VertexLayout;
use ard_log::warn;
use ard_render_base::resource::{ResourceAllocator, ResourceId};
use ard_render_material::material::MaterialResource;
use ard_render_meshes::mesh::MeshResource;
use ard_render_objects::keys::SeparatedDrawKey;

pub struct RenderStateTracker {
    last_material: ResourceId,
    last_vertex_layout: VertexLayout,
    last_data_size: u32,
}

#[derive(Debug, Default)]
pub struct BindingDelta {
    pub skip: bool,
    pub new_material: Option<ResourceId>,
    pub new_vertices: Option<VertexLayout>,
    pub new_data_size: Option<u32>,
}

impl RenderStateTracker {
    pub fn compute_delta<'a, const FIF: usize>(
        &mut self,
        key: &SeparatedDrawKey,
        meshes: &'a ResourceAllocator<MeshResource, FIF>,
        materials: &'a ResourceAllocator<MaterialResource, FIF>,
    ) -> BindingDelta {
        let mut delta = BindingDelta {
            skip: false,
            new_material: None,
            new_vertices: None,
            new_data_size: None,
        };

        // Grab the mesh
        let mesh = match meshes.get(key.mesh_id) {
            Some(mesh) => mesh,
            None => {
                warn!(
                    "Attempt to render with mesh `{:?}` that did not exist. Skipping draw.",
                    key.mesh_id
                );
                delta.skip = true;
                return delta;
            }
        };

        // Skip if the mesh is not ready
        if !mesh.ready {
            delta.skip = true;
            return delta;
        }

        // Grab the material
        let material = match materials.get(key.material_id) {
            Some(material) => material,
            None => {
                warn!(
                    "Attempt to render with material `{:?}` that did not exist. Skipping draw.",
                    key.material_id
                );
                delta.skip = true;
                return delta;
            }
        };

        // We have everything we need. Mark what has changed
        if self.last_material != key.material_id {
            delta.new_material = Some(key.material_id);
            self.last_material = key.material_id;
        }

        if self.last_vertex_layout != mesh.block.layout() {
            delta.new_vertices = Some(mesh.block.layout());
            self.last_vertex_layout = mesh.block.layout();
        }

        if self.last_data_size != material.data_size {
            delta.new_data_size = Some(material.data_size);
            self.last_data_size = material.data_size;
        }

        delta
    }
}

impl Default for RenderStateTracker {
    fn default() -> Self {
        Self {
            last_material: ResourceId::from(usize::MAX),
            last_vertex_layout: VertexLayout::empty(),
            last_data_size: u32::MAX,
        }
    }
}

impl BindingDelta {
    #[inline]
    pub fn draw_required(&self) -> bool {
        self.skip
            || self.new_material.is_some()
            || self.new_vertices.is_some()
            || self.new_data_size.is_some()
    }
}
