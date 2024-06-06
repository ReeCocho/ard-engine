use std::ops::{Div, Shr};

use ard_formats::texture::MipType;
use ard_log::warn;
use ard_pal::prelude::{
    AnisotropyLevel, Blit, BlitDestination, BlitSource, Buffer, BufferTextureCopy, CommandBuffer,
    Context, DescriptorSet, DescriptorSetCreateInfo, DescriptorSetUpdate, DescriptorValue, Filter,
    Format, MemoryUsage, MultiSamples, QueueType, QueueTypes, Sampler, SamplerAddressMode,
    SharingMode, Texture, TextureCreateInfo, TextureType, TextureUsage,
};
use ard_render_base::{
    ecs::Frame,
    resource::{ResourceAllocator, ResourceId},
    FRAMES_IN_FLIGHT,
};
use ard_render_si::{bindings::*, consts::*};
use ordered_float::NotNan;
use rustc_hash::FxHashSet;

use crate::texture::TextureResource;

type PalTexture = ard_pal::prelude::Texture;

pub struct TextureFactory {
    /// Default error texture.
    error_tex: PalTexture,
    /// Current anisotropy level.
    anisotropy: Option<AnisotropyLevel>,
    /// Bindless texture set per frame in flight.
    sets: [DescriptorSet; FRAMES_IN_FLIGHT],
    new_textures: [Vec<ResourceId>; FRAMES_IN_FLIGHT],
    mip_updates: [Vec<MipUpdate>; FRAMES_IN_FLIGHT],
    dropped_textures: [Vec<ResourceId>; FRAMES_IN_FLIGHT],
}

#[derive(Copy, Clone)]
pub enum MipUpdate {
    Texture(ResourceId),
    CubeMap(ResourceId),
}

pub struct TextureUpload {
    pub staging: Buffer,
    pub mip_type: MipType,
    pub loaded_mips: u32,
}

pub struct TextureMipUpload {
    pub staging: Buffer,
    pub mip_level: u32,
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

impl TextureFactory {
    pub fn new(ctx: &Context, layouts: &Layouts) -> Self {
        let error_tex = Self::create_error_texture(ctx);

        let sets = std::array::from_fn(|frame_idx| {
            let mut set = DescriptorSet::new(
                ctx.clone(),
                DescriptorSetCreateInfo {
                    layout: layouts.textures.clone(),
                    debug_name: Some(format!("texture_set_{frame_idx}")),
                },
            )
            .unwrap();

            let updates: Vec<_> = (0..MAX_TEXTURES)
                .map(|slot| DescriptorSetUpdate {
                    binding: TEXTURES_SET_TEXTURES_BINDING,
                    array_element: slot,
                    value: DescriptorValue::Texture {
                        texture: &error_tex,
                        array_element: 0,
                        sampler: DEFAULT_SAMPLER,
                        base_mip: 0,
                        mip_count: 1,
                    },
                })
                .collect();

            set.update(&updates);

            set
        });

        Self {
            sets,
            error_tex,
            anisotropy: Some(AnisotropyLevel::X16),
            new_textures: Default::default(),
            dropped_textures: Default::default(),
            mip_updates: Default::default(),
        }
    }

    #[inline(always)]
    pub fn get_set(&self, frame: Frame) -> &DescriptorSet {
        &self.sets[usize::from(frame)]
    }

    /// Signals a texture is uploaded and ready to be bound.
    #[inline(always)]
    pub fn texture_ready(&mut self, id: ResourceId) {
        self.new_textures.iter_mut().for_each(|l| l.push(id));
    }

    /// Signal that a texture has been dropped and should be removed from the texture set.
    #[inline(always)]
    pub fn texture_dropped(&mut self, id: ResourceId) {
        self.dropped_textures.iter_mut().for_each(|l| l.push(id));
    }

    /// Signal that an image has had a mip level updates and might need rebinding.
    #[inline(always)]
    pub fn mip_update(&mut self, update: MipUpdate) {
        self.mip_updates.iter_mut().for_each(|l| l.push(update));
    }

    /// Binds ready textures to the main set for the given frame and unbinds destroyed textures.
    pub fn update_bindings(&mut self, frame: Frame, textures: &ResourceAllocator<TextureResource>) {
        let frame = usize::from(frame);
        let cap = self.new_textures[frame].len()
            + self.dropped_textures[frame].len()
            + self.mip_updates[frame].len();
        let mut updates = Vec::with_capacity(cap);

        // Hash set of already observed image ids so we don't double update
        let mut observed = FxHashSet::<ResourceId>::default();

        // Replaced dropped textures with the error texture
        self.dropped_textures[frame].drain(..).for_each(|id| {
            if !observed.insert(id) {
                return;
            }

            updates.push(DescriptorSetUpdate {
                binding: 0,
                array_element: usize::from(id),
                value: DescriptorValue::Texture {
                    texture: &self.error_tex,
                    array_element: 0,
                    sampler: DEFAULT_SAMPLER,
                    base_mip: 0,
                    mip_count: 1,
                },
            });
        });

        // Write new textures to the set
        self.new_textures[frame].drain(..).for_each(|id| {
            if !observed.insert(id) {
                return;
            }

            let texture = match textures.get(id) {
                Some(texture) => texture,
                None => {
                    warn!("Attempt to bind new texture `{id:?}` but the resource did not exist.");
                    return;
                }
            };

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
        });

        // Update mips
        self.mip_updates[frame].drain(..).for_each(|update| {
            match update {
                MipUpdate::Texture(id) => {
                    if !observed.insert(id) {
                        return;
                    }

                    let texture = match textures.get(id) {
                        Some(texture) => texture,
                        None => {
                            warn!("Attempt to update texture mip `{id:?}` but the resource did not exist.");
                            return;
                        }
                    };

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
                },
                MipUpdate::CubeMap(_) => todo!(),
            }
        });

        // Perform the update
        self.sets[frame].update(&updates);
    }

    /// Records a command to upload a texture to the factory and generate mip levels.
    ///
    /// ## Note
    /// `commands` must have graphics and transfer operation support.
    pub fn upload_gen_mip<'a>(
        commands: &mut CommandBuffer<'a>,
        texture: &'a Texture,
        mip_count: u32,
        upload: &'a TextureUpload,
    ) {
        // Staging buffer has the highest detail mip level
        commands.copy_buffer_to_texture(
            texture,
            &upload.staging,
            BufferTextureCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                buffer_array_element: 0,
                texture_offset: (0, 0, 0),
                texture_extent: texture.dims(),
                texture_mip_level: 0,
                texture_array_element: 0,
            },
        );

        // Blit each image in the mip chain
        let (mut mip_width, mut mip_height, _) = texture.dims();
        for i in 1..mip_count {
            let width = mip_width.div(2).max(1);
            let height = mip_height.div(2).max(1);
            commands.blit(
                BlitSource::Texture(texture),
                BlitDestination::Texture(texture),
                Blit {
                    src_min: (0, 0, 0),
                    src_max: (mip_width, mip_height, 1),
                    src_mip: (i - 1) as usize,
                    src_array_element: 0,
                    dst_min: (0, 0, 0),
                    dst_max: (width, height, 1),
                    dst_mip: i as usize,
                    dst_array_element: 0,
                },
                Filter::Linear,
            );
            mip_width = width;
            mip_height = height;
        }

        commands.set_texture_usage(texture, TextureUsage::SAMPLED, 0, 0, mip_count as usize);
    }

    /// Records a command to upload a lowest detail mip level of a texture.
    ///
    /// ## Note
    /// `commands` must have transfer operation support.
    pub fn upload<'a>(
        commands: &mut CommandBuffer<'a>,
        texture: &'a Texture,
        mip_level: u32,
        upload: &'a TextureUpload,
    ) {
        let (mut width, mut height, _) = texture.dims();
        width = width.shr(mip_level).max(1);
        height = height.shr(mip_level).max(1);

        // Staging buffer has the lowest detail mip level
        commands.copy_buffer_to_texture(
            texture,
            &upload.staging,
            BufferTextureCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                buffer_array_element: 0,
                texture_offset: (0, 0, 0),
                texture_extent: (width, height, 1),
                texture_mip_level: mip_level as usize,
                texture_array_element: 0,
            },
        );

        commands.set_texture_usage(texture, TextureUsage::SAMPLED, 0, mip_level, 1);

        commands.transfer_texture_ownership(
            texture,
            0,
            mip_level as usize,
            1,
            QueueType::Main,
            Some(TextureUsage::SAMPLED),
        );
    }

    /// Records a command to upload a texture mip level.
    ///
    /// ## Note
    /// `commands` must have transfer operation support.
    pub fn upload_mip<'a>(
        commands: &mut CommandBuffer<'a>,
        texture: &'a Texture,
        upload: &'a TextureMipUpload,
    ) {
        let (width, height, _) = texture.dims();

        commands.copy_buffer_to_texture(
            texture,
            &upload.staging,
            BufferTextureCopy {
                buffer_offset: 0,
                buffer_row_length: 0,
                buffer_image_height: 0,
                buffer_array_element: 0,
                texture_offset: (0, 0, 0),
                texture_extent: (
                    width.shr(upload.mip_level).max(1),
                    height.shr(upload.mip_level).max(1),
                    1,
                ),
                texture_mip_level: upload.mip_level as usize,
                texture_array_element: 0,
            },
        );

        commands.set_texture_usage(texture, TextureUsage::SAMPLED, 0, upload.mip_level, 1);

        commands.transfer_texture_ownership(
            texture,
            0,
            upload.mip_level as usize,
            1,
            QueueType::Main,
            Some(TextureUsage::SAMPLED),
        );
    }

    fn create_error_texture(ctx: &Context) -> PalTexture {
        let staging = Buffer::new_staging(
            ctx.clone(),
            QueueType::Transfer,
            Some("error_texture_staging".to_owned()),
            &[255u8, 0, 255, 255],
        )
        .unwrap();

        let tex = PalTexture::new(
            ctx.clone(),
            TextureCreateInfo {
                format: Format::Rgba8Unorm,
                ty: TextureType::Type2D,
                width: 1,
                height: 1,
                depth: 1,
                array_elements: 1,
                mip_levels: 1,
                sample_count: MultiSamples::Count1,
                texture_usage: TextureUsage::TRANSFER_DST | TextureUsage::SAMPLED,
                memory_usage: MemoryUsage::GpuOnly,
                queue_types: QueueTypes::MAIN | QueueTypes::TRANSFER,
                sharing_mode: SharingMode::Exclusive,
                debug_name: Some("error_texture".to_owned()),
            },
        )
        .unwrap();

        let mut commands = ctx.transfer().command_buffer();
        commands.copy_buffer_to_texture(
            &tex,
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
        commands.set_texture_usage(&tex, TextureUsage::SAMPLED, 0, 0, 1);
        commands.transfer_texture_ownership(
            &tex,
            0,
            0,
            1,
            QueueType::Main,
            Some(TextureUsage::SAMPLED),
        );
        ctx.transfer()
            .submit(Some("error_texture_upload"), commands);

        tex
    }
}
