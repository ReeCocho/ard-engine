pub mod buffer;
pub mod set;

use std::collections::HashMap;

use ard_pal::prelude::{Context, DescriptorSet, DescriptorSetLayout};
use ard_render_base::ecs::Frame;
use ard_render_base::resource::ResourceAllocator;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::material_instance::MaterialInstance;
use crate::material_instance::{MaterialInstanceResource, TextureSlot};
use ard_render_si::consts::{EMPTY_TEXTURE_ID, MAX_TEXTURES_PER_MATERIAL};

use self::buffer::MaterialSlot;
use self::{buffer::MaterialBuffer, set::MaterialSet};

#[derive(Serialize, Deserialize)]
pub struct MaterialFactoryConfig {
    /// Initial capacity for the textures buffer.
    pub default_textures_cap: usize,
    /// Initial capacity for material buffers not defined in `default_materials_cap`.
    pub fallback_materials_cap: usize,
    /// Initial capacities for material buffers keyed by their data size.
    pub default_materials_cap: HashMap<u64, usize>,
}

pub struct MaterialFactory<const FRAMES_IN_FLIGHT: usize> {
    ctx: Context,
    config: MaterialFactoryConfig,
    /// Layout for material sets.
    layout: DescriptorSetLayout,
    /// Passes that materials can handle.
    passes: FxHashMap<PassId, PassDefinition>,
    rt_passes: FxHashMap<PassId, RtPassDefinition>,
    /// Material data buffers keyed by their data size.
    data: FxHashMap<u64, MaterialBuffer<FRAMES_IN_FLIGHT>>,
    /// Global texture slots buffer
    textures: MaterialBuffer<FRAMES_IN_FLIGHT>,
    /// Descriptor sets for materials, keyed by their data size.
    sets: FxHashMap<u64, [MaterialSet; FRAMES_IN_FLIGHT]>,
}

pub struct PassDefinition {
    /// Layouts required for this pass.
    pub layouts: Vec<DescriptorSetLayout>,
    /// If this pass has a depth/stencil attachment.
    pub has_depth_stencil_attachment: bool,
    /// The number of color attachments this pass has.
    pub color_attachment_count: usize,
}

pub struct RtPassDefinition {
    pub layouts: Vec<DescriptorSetLayout>,
    pub push_constant_size: Option<u32>,
    pub max_ray_recursion: u32,
    pub max_ray_payload_size: u32,
    pub max_ray_hit_attribute_size: u32,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PassId(usize);

#[derive(Debug, Error)]
pub enum AddPassError {
    #[error("a pass with id `{0:?}` already exists")]
    AlreadyExists(PassId),
}

impl<const FRAMES_IN_FLIGHT: usize> MaterialFactory<FRAMES_IN_FLIGHT> {
    pub fn new(ctx: Context, layout: DescriptorSetLayout, config: MaterialFactoryConfig) -> Self {
        MaterialFactory {
            passes: FxHashMap::default(),
            rt_passes: FxHashMap::default(),
            data: FxHashMap::default(),
            textures: MaterialBuffer::new(
                ctx.clone(),
                "texture_slots_buffer".to_owned(),
                MAX_TEXTURES_PER_MATERIAL as u64 * std::mem::size_of::<TextureSlot>() as u64,
                config.default_textures_cap,
            ),
            sets: FxHashMap::default(),
            ctx,
            layout,
            config,
        }
    }

    #[inline]
    pub fn get_set(&self, frame: Frame, data_size: u64) -> Option<&DescriptorSet> {
        self.sets
            .get(&data_size)
            .map(|set| set[usize::from(frame)].set())
    }

    #[inline]
    pub fn get_pass(&self, id: PassId) -> Option<&PassDefinition> {
        self.passes.get(&id)
    }

    #[inline]
    pub fn get_rt_pass(&self, id: PassId) -> Option<&RtPassDefinition> {
        self.rt_passes.get(&id)
    }

    pub fn add_pass(&mut self, id: PassId, def: PassDefinition) -> Result<(), AddPassError> {
        if self.passes.contains_key(&id) {
            return Err(AddPassError::AlreadyExists(id));
        }

        self.passes.insert(id, def);
        Ok(())
    }

    pub fn add_rt_pass(&mut self, id: PassId, def: RtPassDefinition) -> Result<(), AddPassError> {
        if self.rt_passes.contains_key(&id) {
            return Err(AddPassError::AlreadyExists(id));
        }

        self.rt_passes.insert(id, def);
        Ok(())
    }

    /// Verifies that a set for a particular data size exists.
    pub fn verify_set(&mut self, data_size: u64) {
        self.sets.entry(data_size).or_insert_with(|| {
            std::array::from_fn(|frame_idx| {
                MaterialSet::new(
                    self.ctx.clone(),
                    self.layout.clone(),
                    data_size,
                    frame_idx.into(),
                )
            })
        });
    }

    pub fn allocate_data_slot(&mut self, data_size: u64) -> MaterialSlot {
        let buffer = self.data.entry(data_size).or_insert_with(|| {
            let cap = *self
                .config
                .default_materials_cap
                .get(&data_size)
                .unwrap_or(&self.config.fallback_materials_cap);
            MaterialBuffer::new(
                self.ctx.clone(),
                format!("material_data_buffer_{data_size}"),
                data_size,
                cap,
            )
        });
        buffer.allocate()
    }

    pub fn allocate_textures_slot(&mut self) -> MaterialSlot {
        self.textures.allocate()
    }

    pub fn free_data_slot(&mut self, data_size: u64, slot: MaterialSlot) {
        self.data
            .get_mut(&data_size)
            .iter_mut()
            .for_each(|buffer| buffer.free(slot));
    }

    pub fn free_textures_slot(&mut self, slot: MaterialSlot) {
        self.textures.free(slot);
    }

    /// Marks a particular material as dirty so it can be written into the material buffer.
    pub fn mark_dirty(&mut self, material: MaterialInstance) {
        if let Some(buffer) = self.data.get_mut(&(material.material().data_size() as u64)) {
            buffer.mark_dirty(&material);
        }

        if material.material().texture_slots() > 0 {
            self.textures.mark_dirty(&material);
        }
    }

    /// Flushes all dirty material buffers to the GPU.
    pub fn flush(
        &mut self,
        frame: Frame,
        materials: &ResourceAllocator<MaterialInstanceResource, FRAMES_IN_FLIGHT>,
        data_binding: u32,
        textures_binding: u32,
    ) {
        let mut need_rebind;

        // Flush textures
        need_rebind = self.textures.flush(frame, materials, |buffer, mat| {
            const DATA_SIZE: usize =
                MAX_TEXTURES_PER_MATERIAL as usize * std::mem::size_of::<TextureSlot>();

            let start = match mat.textures_slot {
                Some(slot) => usize::from(slot) * DATA_SIZE,
                None => return,
            };
            let end = start + DATA_SIZE;
            let slots = bytemuck::cast_slice_mut::<_, u32>(&mut buffer[start..end]);

            for (i, tex) in mat.textures.iter().enumerate() {
                slots[i] = match tex {
                    Some(tex) => usize::from(tex.id()) as u32,
                    None => EMPTY_TEXTURE_ID,
                };
            }
        });

        // Flush every data buffer
        self.data.values_mut().for_each(|buffer| {
            // Flush material data
            let data_size = buffer.data_size() as usize;
            need_rebind |= buffer.flush(frame, materials, |buffer, mat| {
                let start = match mat.data_slot {
                    Some(slot) => usize::from(slot) * data_size,
                    None => return,
                };
                let end = start + data_size;
                buffer[start..end].copy_from_slice(&mat.data);
            });
        });

        // Rebind if needed
        if need_rebind {
            self.sets
                .values_mut()
                .flat_map(|set| set.iter_mut().enumerate())
                .for_each(|(frame, set)| {
                    let material = self.data.get(&set.data_size());
                    set.check_rebind(
                        frame.into(),
                        material,
                        data_binding,
                        &self.textures,
                        textures_binding,
                    )
                });
        }
    }
}

impl PassId {
    pub const fn new(id: usize) -> Self {
        Self(id)
    }
}

impl From<usize> for PassId {
    fn from(value: usize) -> Self {
        PassId(value)
    }
}

impl From<PassId> for usize {
    fn from(value: PassId) -> Self {
        value.0
    }
}
