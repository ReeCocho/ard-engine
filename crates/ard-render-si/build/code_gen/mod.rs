pub mod glsl;
pub mod rust;

use crate::{binding::GpuBinding, constants::GpuConstant, structure::GpuStructFieldType};

pub trait StructCodeGen {
    fn begin_struct(&mut self, name: &str);
    fn write_field(&mut self, name: &str, ty: &GpuStructFieldType);
    fn end_struct(&mut self, name: &str);
}

pub trait DescriptorSetCodeGen {
    fn begin_set(&mut self, name: &str);
    fn write_binding(&mut self, name: &str, binding: &GpuBinding);
    fn end_set(&mut self, name: &str);
}

pub trait ConstantsCodeGen {
    fn write_constant(&mut self, constant: &GpuConstant);
}
