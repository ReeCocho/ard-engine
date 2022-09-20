use crate::{
    command_buffer::Command, compute_pipeline::ComputePipeline, descriptor_set::DescriptorSet,
    types::ShaderStage, Backend,
};

pub struct ComputePass<'a, B: Backend> {
    pub(crate) bound_pipeline: bool,
    pub(crate) commands: Vec<Command<'a, B>>,
}

impl<'a, B: Backend> ComputePass<'a, B> {
    /// Binds a compute pipeline to the scope.
    ///
    /// # Arguments
    /// - `pipeline` - The compute pipeline to bind.
    #[inline]
    pub fn bind_pipeline(&mut self, pipeline: ComputePipeline<B>) {
        self.bound_pipeline = true;
        self.commands.push(Command::BindComputePipeline(pipeline));
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
            stage: ShaderStage::Compute,
        });
    }

    /// Dispatches `x * y * z` local workgroups.
    ///
    /// # Arguments
    /// - `x` - Number of local workgroups in the X dimension.
    /// - `y` - Number of local workgroups in the Y dimension.
    /// - `z` - Number of local workgroups in the Z dimension.
    ///
    /// # Panics
    /// - If there is no bound compute pipeline.
    #[inline]
    pub fn dispatch(&mut self, x: u32, y: u32, z: u32) {
        assert!(self.bound_pipeline, "no bound compute pipeline");
        self.commands.push(Command::Dispatch(x, y, z));
    }
}
