use api::{
    texture::Sampler,
    types::{AnisotropyLevel, Filter},
};
use ash::vk;
use fxhash::FxHashMap;

#[derive(Default)]
pub(crate) struct SamplerCache {
    samplers: FxHashMap<Sampler, vk::Sampler>,
}

impl SamplerCache {
    pub unsafe fn get(&mut self, device: &ash::Device, sampler: Sampler) -> vk::Sampler {
        *self.samplers.entry(sampler).or_insert_with(|| {
            let create_info = vk::SamplerCreateInfo::builder()
                .min_filter(crate::util::to_vk_filter(sampler.min_filter))
                .mag_filter(crate::util::to_vk_filter(sampler.mag_filter))
                .mipmap_mode(match sampler.mipmap_filter {
                    Filter::Nearest => vk::SamplerMipmapMode::NEAREST,
                    Filter::Linear => vk::SamplerMipmapMode::LINEAR,
                })
                .address_mode_u(crate::util::to_vk_address_mode(sampler.address_u))
                .address_mode_v(crate::util::to_vk_address_mode(sampler.address_v))
                .address_mode_w(crate::util::to_vk_address_mode(sampler.address_w))
                .anisotropy_enable(sampler.anisotropy.is_some())
                .max_anisotropy(match sampler.anisotropy {
                    Some(anisotropy) => match anisotropy {
                        AnisotropyLevel::X1 => 1.0,
                        AnisotropyLevel::X2 => 2.0,
                        AnisotropyLevel::X4 => 4.0,
                        AnisotropyLevel::X8 => 8.0,
                        AnisotropyLevel::X16 => 16.0,
                    },
                    None => 0.0,
                })
                .compare_enable(sampler.compare.is_some())
                .compare_op(match sampler.compare {
                    Some(compare) => crate::util::to_vk_compare_op(compare),
                    None => vk::CompareOp::ALWAYS,
                })
                .min_lod(sampler.min_lod.into())
                .max_lod(match sampler.max_lod {
                    Some(max_lod) => max_lod.into(),
                    None => vk::LOD_CLAMP_NONE,
                })
                .unnormalized_coordinates(sampler.unnormalize_coords)
                .build();

            device.create_sampler(&create_info, None).unwrap()
        })
    }

    pub unsafe fn release(&mut self, device: &ash::Device) {
        for (_, sampler) in self.samplers.drain() {
            device.destroy_sampler(sampler, None);
        }
    }
}
