use serde::{Deserialize, Serialize};

use crate::code_gen::ConstantsCodeGen;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuConstant {
    pub name: String,
    pub value: ConstantValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConstantType {
    Float,
    Int,
    UInt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConstantValue {
    Float(f32),
    Int(i32),
    UInt(u32),
    USize(usize),
    Custom(ConstantType, String),
}

impl GpuConstant {
    pub fn gen(&self, gen: &mut impl ConstantsCodeGen) {
        gen.write_constant(self);
    }
}

impl ConstantType {
    pub fn to_rust_type(&self) -> &str {
        match self {
            ConstantType::Float => "f32",
            ConstantType::Int => "i32",
            ConstantType::UInt => "u32",
        }
    }
}
