use std::ops::Range;

use crate::{
    buffer::Buffer, command_buffer::Command, descriptor_set::DescriptorSet, types::ShaderStage,
    Backend,
};

pub struct RayTracingPass<'a, B: Backend> {
    pub(crate) commands: Vec<Command<'a, B>>,
}

pub struct RayTracingDispatch<'a, B: Backend> {
    pub dims: (u32, u32, u32),
    pub shader_binding_table: &'a Buffer<B>,
    pub raygen_offset: u64,
    pub miss_offset: u64,
    pub hit_range: Range<u64>,
}

impl<'a, B: Backend> RayTracingPass<'a, B> {
    #[inline]
    pub fn push_constants(&mut self, data: &[u8]) {
        self.commands.push(Command::PushConstants {
            data: Vec::from(data),
            stage: ShaderStage::RayTracing,
        });
    }

    /// Binds one or more descriptor sets to the scope.
    ///
    /// # Arguments
    /// - `first` - An offset added to the set indices. For example, if you wanted to bind only the
    /// second set of your pipeline, you would set `first = 1`.
    /// - `sets` - The sets to bind.
    ///
    /// # Panics
    /// - If `sets.is_empty()`.
    ///
    /// # Valid Usage
    /// The user *must* ensure that the bound sets do not go out of bounds of the pipeline they are
    /// used in. Backends *should* perform validity checking of set bounds.
    #[inline]
    pub fn bind_sets(&mut self, first: usize, sets: Vec<&'a DescriptorSet<B>>) {
        assert!(!sets.is_empty(), "no sets provided");
        self.commands.push(Command::BindDescriptorSets {
            sets,
            first,
            stage: ShaderStage::RayTracing,
        });
    }
}
