use std::sync::Arc;
use thiserror::Error;

use crate::{
    buffer::Buffer,
    context::Context,
    texture::{Sampler, Texture},
    types::{AccessType, ShaderStage},
    Backend,
};

pub struct DescriptorSetCreateInfo<B: Backend> {
    /// The layout to create the set with.
    pub layout: DescriptorSetLayout<B>,
    /// The backend *should* use the provided debug name for easy identification.
    pub debug_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DescriptorSetLayoutCreateInfo {
    /// The bindings of this set.
    pub bindings: Vec<DescriptorBinding>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DescriptorBinding {
    /// The index of the binding within the set.
    pub binding: u32,
    /// Type of object held within this binding.
    pub ty: DescriptorType,
    /// The number of array elements for this binding.
    pub count: usize,
    /// The shader stages that have access to this binding.
    pub stage: ShaderStage,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DescriptorType {
    /// A read-only sampled texture.
    Texture,
    /// A read-only uniform buffer object.
    UniformBuffer,
    /// A read-only or read-write storage buffer object.
    StorageBuffer(AccessType),
    /// A read-only or read-write storage image object.
    StorageImage(AccessType),
}

#[derive(Debug, Error)]
pub enum DescriptorSetLayoutCreateError {
    #[error("an error has occured: {0}")]
    Other(String),
}

#[derive(Debug, Error)]
pub enum DescriptorSetCreateError {
    #[error("an error has occured: {0}")]
    Other(String),
}

pub struct DescriptorSetLayout<B: Backend>(Arc<DescriptorSetLayoutInner<B>>);

pub struct DescriptorSet<B: Backend> {
    ctx: Context<B>,
    layout: DescriptorSetLayout<B>,
    pub(crate) id: B::DescriptorSet,
}

pub struct DescriptorSetUpdate<'a, B: Backend> {
    /// The binding to update within the set.
    pub binding: u32,
    /// The array element within the binding to update.
    pub array_element: usize,
    /// The value to update the binding with.
    pub value: DescriptorValue<'a, B>,
}

pub enum DescriptorValue<'a, B: Backend> {
    UniformBuffer {
        /// The uniform buffer to bind.
        buffer: &'a Buffer<B>,
        /// The array element of the uniform buffer to bind.
        array_element: usize,
    },
    StorageBuffer {
        /// The storage buffer to bind.
        buffer: &'a Buffer<B>,
        /// The array element of the storage buffer to bind.
        array_element: usize,
    },
    StorageImage {
        /// The texture to bind.
        texture: &'a Texture<B>,
        /// The array element of the texture to bind.
        array_element: usize,
        /// The mip level of the texture to bind.
        mip: usize,
    },
    Texture {
        /// The texture to bind.
        texture: &'a Texture<B>,
        /// The array element of the texture to bind.
        array_element: usize,
        /// How the texture should be sampled.
        sampler: Sampler,
        /// The base mip to bind.
        base_mip: usize,
        /// The number of mip levels to bind.
        mip_count: usize,
    },
}

pub(crate) struct DescriptorSetLayoutInner<B: Backend> {
    ctx: Context<B>,
    pub(crate) id: B::DescriptorSetLayout,
}

impl<B: Backend> DescriptorSet<B> {
    /// Creates a new descriptor set.
    ///
    /// # Arguments
    /// - `ctx` - The [`Context`] to create the buffer with.
    /// - `create_info` - Describes the descriptor set to create.
    #[inline(always)]
    pub fn new(
        ctx: Context<B>,
        create_info: DescriptorSetCreateInfo<B>,
    ) -> Result<Self, DescriptorSetCreateError> {
        let layout = create_info.layout.clone();
        let id = unsafe { ctx.0.create_descriptor_set(create_info)? };
        Ok(Self { ctx, layout, id })
    }

    #[inline(always)]
    pub fn internal(&self) -> &B::DescriptorSet {
        &self.id
    }

    #[inline(always)]
    pub fn layout(&self) -> &DescriptorSetLayout<B> {
        &self.layout
    }

    /// Updates the descriptor set with new values.
    ///
    /// # Arguments
    /// - `updates` - The updates to perform on the set.
    ///
    /// # Panics
    /// TODO
    ///
    /// # Synchronization
    /// The backend *must* ensure that the descriptor set is not being accessed by any queue at the
    /// time of the update.
    pub fn update(&mut self, updates: &[DescriptorSetUpdate<B>]) {
        unsafe {
            self.ctx
                .0
                .update_descriptor_sets(&mut self.id, &self.layout.0.id, updates);
        }
    }
}

impl<B: Backend> Drop for DescriptorSet<B> {
    #[inline(always)]
    fn drop(&mut self) {
        unsafe {
            self.ctx.0.destroy_descriptor_set(&mut self.id);
        }
    }
}

impl<B: Backend> DescriptorSetLayout<B> {
    #[inline(always)]
    pub fn new(
        ctx: Context<B>,
        create_info: DescriptorSetLayoutCreateInfo,
    ) -> Result<Self, DescriptorSetLayoutCreateError> {
        let id = unsafe { ctx.0.create_descriptor_set_layout(create_info)? };
        Ok(Self(Arc::new(DescriptorSetLayoutInner { ctx, id })))
    }

    #[inline(always)]
    pub fn internal(&self) -> &B::DescriptorSetLayout {
        &self.0.id
    }
}

impl<B: Backend> Clone for DescriptorSetLayout<B> {
    #[inline(always)]
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<B: Backend> Drop for DescriptorSetLayoutInner<B> {
    #[inline(always)]
    fn drop(&mut self) {
        unsafe {
            self.ctx.0.destroy_descriptor_set_layout(&mut self.id);
        }
    }
}
