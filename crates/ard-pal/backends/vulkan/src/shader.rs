use std::ffi::CString;

use api::shader::{ShaderCreateError, ShaderCreateInfo};
use ash::vk;

pub struct Shader {
    pub(crate) module: vk::ShaderModule,
}

impl Shader {
    pub(crate) unsafe fn new(
        device: &ash::Device,
        debug: Option<&ash::ext::debug_utils::Device>,
        create_info: ShaderCreateInfo,
    ) -> Result<Self, ShaderCreateError> {
        if create_info.code.len() % std::mem::size_of::<u32>() != 0 {
            return Err(ShaderCreateError::Other(String::from(
                "shader code size is not a multiple of 4",
            )));
        }

        let module_create_info = vk::ShaderModuleCreateInfo {
            p_code: create_info.code.as_ptr() as *const u32,
            code_size: create_info.code.len(),
            ..Default::default()
        };
        let module = match device.create_shader_module(&module_create_info, None) {
            Ok(module) => module,
            Err(err) => return Err(ShaderCreateError::Other(err.to_string())),
        };

        // Name the shader if needed
        if let Some(name) = create_info.debug_name {
            if let Some(debug) = debug {
                let name = CString::new(name).unwrap();
                let name_info = vk::DebugUtilsObjectNameInfoEXT::default()
                    .object_handle(module)
                    .object_name(&name);

                debug.set_debug_utils_object_name(&name_info).unwrap();
            }
        }

        Ok(Shader { module })
    }
}
