use ard_formats::mesh::VertexLayout;
use ard_log::warn;
use ard_pal::prelude::{Buffer, RenderPass};
use ard_render_base::{
    ecs::Frame,
    resource::{ResourceAllocator, ResourceId},
};
use ard_render_camera::ubo::CameraUbo;
use ard_render_material::{
    factory::PassId,
    material::{MaterialResource, MaterialVariant, MaterialVariantRequest},
};
use ard_render_meshes::{factory::MeshFactory, mesh::MeshResource};
use ard_render_objects::{keys::SeparatedDrawKey, set::DrawGroup};
use ard_render_si::types::GpuDrawCall;

pub struct RenderStateTracker {
    last_material: ResourceId,
    last_variant: u32,
    last_mat_vertex_layout: VertexLayout,
    last_mesh_vertex_layout: VertexLayout,
    last_data_size: u32,
}

pub struct RenderArgs<'a, 'b, const FIF: usize> {
    pub pass_id: PassId,
    pub frame: Frame,
    pub pass: &'b mut RenderPass<'a>,
    pub camera: &'a CameraUbo,
    pub mesh_factory: &'a MeshFactory,
    pub meshes: &'a ResourceAllocator<MeshResource, FIF>,
    pub materials: &'a ResourceAllocator<MaterialResource, FIF>,
}

struct BindingDelta<'a> {
    pub skip: bool,
    pub new_material: Option<&'a MaterialVariant>,
    pub new_vertices: Option<&'a MeshResource>,
    pub new_data_size: Option<u32>,
}

impl RenderStateTracker {
    pub fn render_groups<'a, 'b, const FIF: usize>(
        &mut self,
        mut draw_offset: usize,
        args: RenderArgs<'b, '_, FIF>,
        draw_calls: &'b Buffer,
        draw_calls_idx: usize,
        groups: impl Iterator<Item = (usize, &'a DrawGroup)>,
    ) {
        // Reset state for the new group
        *self = Self::default();

        let mut draw_count = 0;

        for (group_idx, group) in groups {
            let key = group.key.separate();
            let delta = self.compute_delta(&key, &args);

            // Perform a draw call if needed
            if delta.draw_required() {
                if draw_count > 0 {
                    args.pass.draw_indexed_indirect(
                        draw_calls,
                        draw_calls_idx,
                        (draw_offset * std::mem::size_of::<GpuDrawCall>()) as u64,
                        draw_count,
                        std::mem::size_of::<GpuDrawCall>() as u64,
                    );
                }

                // If we were told to skip the current group, do so
                draw_count = 0;
                if delta.skip {
                    draw_offset = group_idx + 1;
                    continue;
                } else {
                    draw_offset = group_idx;
                }
            }

            // Perform rebindings
            if let Some(new_material) = delta.new_material {
                args.pass.bind_pipeline(new_material.pipeline.clone());

                // Bind global sets
                args.pass
                    .bind_sets(0, vec![args.camera.get_set(args.frame)]);
            }

            if let Some(new_data_size) = delta.new_data_size {
                if new_data_size > 0 {
                    // TODO: Bind sets
                }
            }

            if let Some(new_vertices) = delta.new_vertices {
                // NOTE: Vertex buffer type must exist if we have a valid mesh that uses it's layout
                let vbuffer = args
                    .mesh_factory
                    .get_vertex_buffer(new_vertices.block.layout())
                    .unwrap();
                vbuffer
                    .bind(args.pass, self.last_mat_vertex_layout)
                    .unwrap();
            }

            draw_count += 1;
        }

        // Perform a final draw if required
        if draw_count > 0 {
            args.pass.draw_indexed_indirect(
                draw_calls,
                draw_calls_idx,
                (draw_offset * std::mem::size_of::<GpuDrawCall>()) as u64,
                draw_count,
                std::mem::size_of::<GpuDrawCall>() as u64,
            );
        }
    }

    fn compute_delta<'a, const FIF: usize>(
        &mut self,
        key: &SeparatedDrawKey,
        args: &RenderArgs<'a, '_, FIF>,
    ) -> BindingDelta<'a> {
        let mut delta = BindingDelta {
            skip: false,
            new_material: None,
            new_vertices: None,
            new_data_size: None,
        };

        // Grab the mesh
        let mesh = match args.meshes.get(key.mesh_id) {
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
        let material = match args.materials.get(key.material_id) {
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

        // Grab the material variant needed
        let variant = match material.get_variant(MaterialVariantRequest {
            pass_id: args.pass_id,
            vertex_layout: key.vertex_layout,
        }) {
            Some(variant) => variant,
            None => {
                warn!(
                    "Attempt to render material `{:?}` with vertex layout `{:?}` but \
                    there were no supported variants. Skipping draw.",
                    key.material_id,
                    mesh.block.layout()
                );
                delta.skip = true;
                return delta;
            }
        };

        // We have everything we need. Mark what has changed
        if self.last_material != key.material_id || self.last_variant != variant.id {
            delta.new_material = Some(variant);
            self.last_material = key.material_id;
            self.last_variant = variant.id;
        }

        if self.last_mesh_vertex_layout != mesh.block.layout() {
            delta.new_vertices = Some(mesh);
            self.last_mesh_vertex_layout = mesh.block.layout();
        }

        if self.last_mat_vertex_layout != variant.vertex_layout {
            delta.new_vertices = Some(mesh);
            self.last_mat_vertex_layout = variant.vertex_layout;
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
            last_mat_vertex_layout: VertexLayout::empty(),
            last_mesh_vertex_layout: VertexLayout::empty(),
            last_data_size: u32::MAX,
            last_variant: u32::MAX,
        }
    }
}

impl<'a> BindingDelta<'a> {
    #[inline]
    pub fn draw_required(&self) -> bool {
        self.skip
            || self.new_material.is_some()
            || self.new_vertices.is_some()
            || self.new_data_size.is_some()
    }
}
