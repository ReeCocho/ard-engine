use std::collections::HashSet;

use ard_pal::prelude::{
    AnisotropyLevel, Buffer, BufferTextureCopy, Context, DescriptorBinding, DescriptorSet,
    DescriptorSetCreateInfo, DescriptorSetLayout, DescriptorSetLayoutCreateInfo,
    DescriptorSetUpdate, DescriptorType, DescriptorValue, Filter, MemoryUsage, Sampler,
    SamplerAddressMode, ShaderStage, TextureCreateInfo, TextureFormat, TextureType, TextureUsage,
};
use ordered_float::NotNan;

use crate::{shader_constants::FRAMES_IN_FLIGHT, texture::TextureInner};

use super::{
    allocator::{ResourceAllocator, ResourceId},
    MAX_TEXTURES,
};

pub(crate) struct TextureSets {
    ctx: Context,
    error_texture: ard_pal::prelude::Texture,
    anisotropy: Option<AnisotropyLevel>,
    layout: DescriptorSetLayout,
    sets: Vec<DescriptorSet>,
    new_textures: [Vec<ResourceId>; FRAMES_IN_FLIGHT],
    mip_updates: [Vec<MipUpdate>; FRAMES_IN_FLIGHT],
    dropped_textures: [Vec<ResourceId>; FRAMES_IN_FLIGHT],
}

#[derive(Copy, Clone)]
pub(crate) enum MipUpdate {
    Texture(ResourceId),
    CubeMap(ResourceId),
}

const DEFAULT_SAMPLER: Sampler = Sampler {
    min_filter: Filter::Nearest,
    mag_filter: Filter::Nearest,
    mipmap_filter: Filter::Nearest,
    address_u: SamplerAddressMode::ClampToEdge,
    address_v: SamplerAddressMode::ClampToEdge,
    address_w: SamplerAddressMode::ClampToEdge,
    anisotropy: None,
    compare: None,
    min_lod: unsafe { NotNan::new_unchecked(0.0) },
    max_lod: None,
    unnormalize_coords: false,
    border_color: None,
};

impl TextureSets {
    pub fn new(ctx: Context, anisotropy: Option<AnisotropyLevel>) -> Self {
        // Create error texture
        let staging = Buffer::new_staging(
            ctx.clone(),
            Some(String::from("error_texture_staging")),
            &[255u8, 0, 255, 255],
        )
        .unwrap();

        let error_texture = ard_pal::prelude::Texture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: TextureFormat::Rgba8Unorm,
                ty: TextureType::Type2D,
                width: 1,
                height: 1,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                texture_usage: TextureUsage::TRANSFER_DST | TextureUsage::SAMPLED,
                memory_usage: MemoryUsage::GpuOnly,
                debug_name: Some(String::from("error_texture")),
            },
        )
        .unwrap();

        let mut commands = ctx.transfer().command_buffer();
        commands.copy_buffer_to_texture(
            &error_texture,
            &staging,
            BufferTextureCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                buffer_array_element: 0,
                texture_offset: (0, 0, 0),
                texture_extent: (1, 1, 1),
                texture_mip_level: 0,
                texture_array_element: 0,
            },
        );
        ctx.transfer()
            .submit(Some("error_texture_upload"), commands);

        // Create texture layout
        let layout = DescriptorSetLayout::new(
            ctx.clone(),
            DescriptorSetLayoutCreateInfo {
                bindings: vec![DescriptorBinding {
                    binding: 0,
                    ty: DescriptorType::Texture,
                    count: MAX_TEXTURES,
                    stage: ShaderStage::AllGraphics,
                }],
            },
        )
        .unwrap();

        // Setup descriptor sets
        let mut sets = Vec::with_capacity(FRAMES_IN_FLIGHT);
        for i in 0..FRAMES_IN_FLIGHT {
            let mut set = DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: layout.clone(),
                    debug_name: Some(format!("texture_set_{}", i)),
                },
            )
            .unwrap();

            let mut updates = Vec::with_capacity(MAX_TEXTURES);
            for i in 0..MAX_TEXTURES {
                updates.push(DescriptorSetUpdate {
                    binding: 0,
                    array_element: i,
                    value: DescriptorValue::Texture {
                        texture: &error_texture,
                        array_element: 0,
                        sampler: DEFAULT_SAMPLER,
                        base_mip: 0,
                        mip_count: 1,
                    },
                });
            }

            set.update(&updates);
            sets.push(set);
        }

        Self {
            ctx,
            error_texture,
            anisotropy,
            layout,
            sets,
            new_textures: Default::default(),
            mip_updates: Default::default(),
            dropped_textures: Default::default(),
        }
    }

    #[inline(always)]
    pub fn layout(&self) -> &DescriptorSetLayout {
        &self.layout
    }

    #[inline(always)]
    pub fn anisotropy(&self) -> Option<AnisotropyLevel> {
        self.anisotropy
    }

    #[inline(always)]
    pub fn set(&self, frame: usize) -> &DescriptorSet {
        &self.sets[frame]
    }

    pub fn set_anisotropy(
        &mut self,
        anisotropy: Option<AnisotropyLevel>,
        textures: &ResourceAllocator<TextureInner>,
    ) {
        if self.anisotropy == anisotropy {
            return;
        }
        self.anisotropy = anisotropy;

        // Find all textures with anisotropy and rebind them.
        for (id, texture) in textures.all().iter().enumerate() {
            if let Some(texture) = texture {
                if texture.sampler.anisotropy {
                    self.texture_ready(ResourceId(id));
                }
            }
        }
    }

    /// Signals a texture is uploaded and ready to be bound.
    #[inline(always)]
    pub fn texture_ready(&mut self, id: ResourceId) {
        for list in &mut self.new_textures {
            list.push(id);
        }
    }

    /// Signal that a texture has been dropped and should be removed from the texture set.
    #[inline(always)]
    pub fn texture_dropped(&mut self, id: ResourceId) {
        for list in &mut self.dropped_textures {
            list.push(id);
        }
    }

    /// Signal that an image has had a mip level updates and might need rebinding.
    #[inline(always)]
    pub fn mip_update(&mut self, update: MipUpdate) {
        for list in &mut self.mip_updates {
            list.push(update);
        }
    }

    /// Binds ready textures to the main set for the given frame and unbinds destroyed textures.
    pub fn update_set(&mut self, frame: usize, textures: &ResourceAllocator<TextureInner>) {
        let cap = self.new_textures[frame].len()
            + self.dropped_textures[frame].len()
            + self.mip_updates[frame].len();
        let mut updates = Vec::with_capacity(cap);

        // Hash set of already observed image ids so we don't double update
        let mut observed = HashSet::<ResourceId>::default();

        // Write new textures to the set
        for id in self.new_textures[frame].drain(..) {
            if observed.contains(&id) {
                continue;
            }

            observed.insert(id);

            if let Some(texture) = textures.get(id) {
                let (base_mip, mip_count) = texture.loaded_mips();
                updates.push(DescriptorSetUpdate {
                    binding: 0,
                    array_element: usize::from(id),
                    value: DescriptorValue::Texture {
                        texture: &texture.texture,
                        array_element: 0,
                        sampler: Sampler {
                            min_filter: texture.sampler.min_filter,
                            mag_filter: texture.sampler.mag_filter,
                            mipmap_filter: texture.sampler.mipmap_filter,
                            address_u: texture.sampler.address_u,
                            address_v: texture.sampler.address_v,
                            address_w: SamplerAddressMode::ClampToEdge,
                            anisotropy: if texture.sampler.anisotropy {
                                self.anisotropy
                            } else {
                                None
                            },
                            compare: None,
                            min_lod: NotNan::new(0.0).unwrap(),
                            max_lod: None,
                            unnormalize_coords: false,
                            border_color: None,
                        },
                        base_mip: base_mip as usize,
                        mip_count: mip_count as usize,
                    },
                });
            }
        }

        // Update mips
        for update in self.mip_updates[frame].drain(..) {
            match update {
                MipUpdate::Texture(id) => {
                    if observed.contains(&id) {
                        continue;
                    }

                    observed.insert(id);

                    if let Some(texture) = textures.get(id) {
                        let (base_mip, mip_count) = texture.loaded_mips();
                        updates.push(DescriptorSetUpdate {
                            binding: 0,
                            array_element: usize::from(id),
                            value: DescriptorValue::Texture {
                                texture: &texture.texture,
                                array_element: 0,
                                sampler: Sampler {
                                    min_filter: texture.sampler.min_filter,
                                    mag_filter: texture.sampler.mag_filter,
                                    mipmap_filter: texture.sampler.mipmap_filter,
                                    address_u: texture.sampler.address_u,
                                    address_v: texture.sampler.address_v,
                                    address_w: SamplerAddressMode::ClampToEdge,
                                    anisotropy: if texture.sampler.anisotropy {
                                        self.anisotropy
                                    } else {
                                        None
                                    },
                                    compare: None,
                                    min_lod: NotNan::new(0.0).unwrap(),
                                    max_lod: None,
                                    unnormalize_coords: false,
                                    border_color: None,
                                },
                                base_mip: base_mip as usize,
                                mip_count: mip_count as usize,
                            },
                        });
                    }
                }
                MipUpdate::CubeMap(_) => {}
            }
        }

        // Replaced dropped textures with the error texture
        for id in self.dropped_textures[frame].drain(..) {
            updates.push(DescriptorSetUpdate {
                binding: 0,
                array_element: usize::from(id),
                value: DescriptorValue::Texture {
                    texture: &self.error_texture,
                    array_element: 0,
                    sampler: DEFAULT_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            });
        }

        self.sets[frame].update(&updates);
    }
}
