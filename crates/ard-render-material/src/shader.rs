use ard_pal::prelude::Context;
use ard_render_base::resource::{ResourceHandle, ResourceId};
use thiserror::Error;

pub struct ShaderCreateInfo<'a> {
    /// Shader code for the module.
    pub code: &'a [u8],
    /// Name to help indentify the shader when debugging.
    pub debug_name: Option<String>,
    /// The number of texture slots this shader supports.
    pub texture_slots: usize,
    /// The size of per instance data this shader supports.
    pub data_size: usize,
    /// Work group size used by mesh, task, and compute shaders. Unused by others.
    pub work_group_size: (u32, u32, u32),
}

#[derive(Debug, Error)]
pub enum ShaderCreateError {
    #[error("gpu error: {0}")]
    GpuError(ard_pal::prelude::ShaderCreateError),
}

#[derive(Clone)]
pub struct Shader {
    handle: ResourceHandle,
    texture_slots: usize,
    data_size: usize,
}

pub struct ShaderResource {
    /// The actual shader module.
    pub shader: ard_pal::prelude::Shader,
    /// Workgroup size for compute, mesh, and task shaders. Unused by other types.
    pub work_group_size: (u32, u32, u32),
    pub texture_slots: usize,
    pub data_size: usize,
}

impl Shader {
    pub fn new(handle: ResourceHandle, texture_slots: usize, data_size: usize) -> Self {
        Shader {
            handle,
            texture_slots,
            data_size,
        }
    }

    #[inline(always)]
    pub fn id(&self) -> ResourceId {
        self.handle.id()
    }

    #[inline(always)]
    pub fn texture_slots(&self) -> usize {
        self.texture_slots
    }

    #[inline(always)]
    pub fn data_size(&self) -> usize {
        self.data_size
    }
}

impl ShaderResource {
    pub fn new(
        mut create_info: ShaderCreateInfo,
        ctx: &Context,
    ) -> Result<Self, ShaderCreateError> {
        let shader = ard_pal::prelude::Shader::new(
            ctx.clone(),
            ard_pal::prelude::ShaderCreateInfo {
                code: create_info.code,
                debug_name: create_info.debug_name.take(),
            },
        )?;

        Ok(ShaderResource {
            shader,
            texture_slots: create_info.texture_slots,
            data_size: create_info.data_size,
            work_group_size: create_info.work_group_size,
        })
    }
}

impl From<ard_pal::prelude::ShaderCreateError> for ShaderCreateError {
    fn from(value: ard_pal::prelude::ShaderCreateError) -> Self {
        ShaderCreateError::GpuError(value)
    }
}
