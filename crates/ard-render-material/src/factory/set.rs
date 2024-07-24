use ard_pal::prelude::{
    Context, DescriptorSet, DescriptorSetCreateInfo, DescriptorSetLayout, DescriptorSetUpdate,
    DescriptorValue,
};
use ard_render_base::Frame;

use super::buffer::MaterialBuffer;

pub struct MaterialSet {
    /// The actual descriptor set.
    set: DescriptorSet,
    /// The data size bound to this set.
    data_size: u64,
    /// The last recorded size of the material buffer. Used to detect rebinding.
    last_buffer_size: u64,
    /// The last recorded size of the texture buffer. Used to detect rebinding.
    last_texture_size: u64,
}

impl MaterialSet {
    pub fn new(ctx: Context, layout: DescriptorSetLayout, data_size: u64, frame: Frame) -> Self {
        MaterialSet {
            set: DescriptorSet::new(
                ctx,
                DescriptorSetCreateInfo {
                    layout,
                    debug_name: Some(format!("material_set_({data_size})_{frame:?}")),
                },
            )
            .unwrap(),
            data_size,
            last_buffer_size: 0,
            last_texture_size: 0,
        }
    }

    #[inline(always)]
    pub fn data_size(&self) -> u64 {
        self.data_size
    }

    #[inline(always)]
    pub fn set(&self) -> &DescriptorSet {
        &self.set
    }

    pub fn check_rebind(
        &mut self,
        frame: Frame,
        material: Option<&MaterialBuffer>,
        material_binding: u32,
        textures: &MaterialBuffer,
        texture_binding: u32,
    ) {
        // Update materials if provided
        if let Some(material) = material {
            let buffer = material.buffer();
            if buffer.size() > self.last_buffer_size {
                self.set.update(&[DescriptorSetUpdate {
                    binding: material_binding,
                    array_element: 0,
                    value: DescriptorValue::StorageBuffer {
                        buffer,
                        array_element: frame.into(),
                    },
                }]);
                self.last_buffer_size = buffer.size();
            }
        }

        // Update textures
        let buffer = textures.buffer();
        if buffer.size() > self.last_texture_size {
            self.set.update(&[DescriptorSetUpdate {
                binding: texture_binding,
                array_element: 0,
                value: DescriptorValue::StorageBuffer {
                    buffer,
                    array_element: frame.into(),
                },
            }]);
            self.last_texture_size = buffer.size();
        }
    }
}
