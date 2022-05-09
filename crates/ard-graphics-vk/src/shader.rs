use crate::{camera::container::EscapeHandle, prelude::*};
use ard_graphics_api::prelude::*;

use ash::vk;

#[derive(Clone)]
pub struct Shader {
    pub(crate) id: u32,
    pub(crate) ty: ShaderType,
    pub(crate) vertex_layout: Option<VertexLayout>,
    pub(crate) inputs: ShaderInputs,
    pub(crate) escaper: EscapeHandle,
}

pub(crate) struct ShaderInner {
    pub ctx: GraphicsContext,
    pub module: vk::ShaderModule,
    pub ty: ShaderType,
    pub inputs: ShaderInputs,
    pub vertex_layout: Option<VertexLayout>,
}

impl ShaderInner {
    pub unsafe fn new(ctx: &GraphicsContext, create_info: &ShaderCreateInfo) -> Self {
        assert!(
            !create_info.code.is_empty()
                && create_info.code.len() % std::mem::size_of::<u32>() == 0
        );

        let vertex_layout = if create_info.ty == ShaderType::Vertex {
            Some(create_info.vertex_layout)
        } else {
            None
        };

        let module_create_info = vk::ShaderModuleCreateInfo {
            p_code: create_info.code.as_ptr() as *const u32,
            code_size: create_info.code.len(),
            ..Default::default()
        };

        let module = ctx
            .0
            .device
            .create_shader_module(&module_create_info, None)
            .expect("unable to compile shader module");

        ShaderInner {
            ctx: ctx.clone(),
            module,
            ty: create_info.ty,
            inputs: create_info.inputs,
            vertex_layout,
        }
    }
}

impl ShaderApi for Shader {
    fn ty(&self) -> ShaderType {
        self.ty
    }

    fn inputs(&self) -> &ShaderInputs {
        &self.inputs
    }
}

impl Drop for ShaderInner {
    fn drop(&mut self) {
        unsafe {
            self.ctx.0.device.destroy_shader_module(self.module, None);
        }
    }
}
