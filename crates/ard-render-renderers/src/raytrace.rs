use ard_math::{Vec3A, Vec4, Vec4Swizzles};
use ard_pal::prelude::*;
use ard_render_base::{ecs::Frame, resource::ResourceAllocator};
use ard_render_meshes::mesh::MeshResource;
use ard_render_objects::{
    objects::RenderObjects,
    set::{RenderableSet, RenderableSetUpdate},
};
use ard_render_si::types::*;

const DEFAULT_OBJECTS_CAP: usize = 1;
const DEFAULT_SCRATCH_SIZE: u64 = 1024;

pub struct RaytracedRenderer {
    ctx: Context,
    frames_in_flight: usize,
    scratch_buffer: Buffer,
    objects: Buffer,
    tlas: TopLevelAccelerationStructure,
    set: RenderableSet,
    capacity: usize,
}

impl RaytracedRenderer {
    pub fn new(ctx: &Context, frames_in_flight: usize) -> Self {
        let tlas = Self::create_tlas(ctx, DEFAULT_OBJECTS_CAP);

        Self {
            ctx: ctx.clone(),
            objects: Self::create_object_buffer(ctx, DEFAULT_OBJECTS_CAP, frames_in_flight),
            scratch_buffer: Self::create_scratch_buffer(
                ctx,
                tlas.scratch_buffer_size().max(DEFAULT_SCRATCH_SIZE),
            ),
            tlas,
            capacity: DEFAULT_OBJECTS_CAP,
            set: RenderableSet::default(),
            frames_in_flight,
        }
    }

    #[inline(always)]
    pub fn tlas(&self) -> &TopLevelAccelerationStructure {
        &self.tlas
    }

    pub fn upload<const FIF: usize>(
        &mut self,
        frame: Frame,
        view_location: Vec3A,
        objects: &RenderObjects,
        meshes: &ResourceAllocator<MeshResource, FIF>,
    ) {
        fn is_visible(bounding_sphere: Vec4, view_location: Vec3A) -> bool {
            const EPSILON: f32 = 0.00001;

            // TODO: Make this configurable
            let ang_cutoff = (14.0_f32).to_radians().tan();

            let d = (view_location - Vec3A::from(bounding_sphere.xyz())).length();
            let r = bounding_sphere.w;

            if d.abs() <= EPSILON {
                return true;
            }

            let tan_theta = r / d;

            tan_theta > ang_cutoff
        }

        // Update the set
        RenderableSetUpdate::new(&mut self.set)
            .with_opaque()
            .with_alpha_cutout()
            .with_transparent()
            .update(
                view_location,
                objects,
                meshes,
                |idx| is_visible(idx.bounding_sphere, view_location),
                |idx| is_visible(idx.bounding_sphere, view_location),
                |idx| is_visible(idx.bounding_sphere, view_location),
            );

        // Resize if we're over capacity
        if self.set.ids().len() > self.capacity {
            // Compute new capacity
            let mut new_cap = self.capacity;
            while new_cap < self.set.ids().len() {
                new_cap *= 2;
            }

            // Resize objects
            self.objects = Self::create_object_buffer(&self.ctx, new_cap, self.frames_in_flight);
            self.tlas = Self::create_tlas(&self.ctx, new_cap);
            self.scratch_buffer =
                Self::create_scratch_buffer(&self.ctx, self.tlas.scratch_buffer_size());
            self.capacity = new_cap;
        }

        // Base address of the object data
        let base = objects.object_data().device_ref(0);

        // Write in object pointers from object data
        let mut view = self.objects.write(usize::from(frame)).unwrap();
        self.set.ids().iter().enumerate().for_each(|(i, id)| {
            view.set_as_array(
                // Offset is equal to the objects ID
                base + (id.data_idx as u64 * std::mem::size_of::<GpuObjectData>() as u64),
                i,
            );
        });
    }

    pub fn build<'a>(&'a self, commands: &mut CommandBuffer<'a>, frame: Frame) {
        commands.build_top_level_acceleration_structure(
            &self.tlas,
            self.set.ids().len(),
            &self.scratch_buffer,
            0,
            &self.objects,
            usize::from(frame),
        );
    }

    fn create_scratch_buffer(ctx: &Context, size: u64) -> Buffer {
        Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size,
                array_elements: 1,
                buffer_usage: BufferUsage::ACCELERATION_STRUCTURE_SCRATCH,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("tlas_scratch".into()),
            },
        )
        .unwrap()
    }

    fn create_object_buffer(ctx: &Context, cap: usize, frames_in_flight: usize) -> Buffer {
        Buffer::new(
            ctx.clone(),
            BufferCreateInfo {
                size: (cap * std::mem::size_of::<u64>()) as u64,
                array_elements: frames_in_flight,
                buffer_usage: BufferUsage::ACCELERATION_STRUCTURE_READ
                    | BufferUsage::STORAGE_BUFFER,
                memory_usage: MemoryUsage::CpuToGpu,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("rt_object_ptrs".into()),
            },
        )
        .unwrap()
    }

    fn create_tlas(ctx: &Context, cap: usize) -> TopLevelAccelerationStructure {
        TopLevelAccelerationStructure::new(
            ctx.clone(),
            TopLevelAccelerationStructureCreateInfo {
                flags: BuildAccelerationStructureFlags::PREFER_FAST_TRACE,
                capacity: cap,
                queue_types: QueueTypes::MAIN,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("rt_tlas".into()),
            },
        )
        .unwrap()
    }
}
