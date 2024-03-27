use convert_case::{Case, Casing};

use super::{ConstantsCodeGen, DescriptorSetCodeGen, StructCodeGen};
use crate::{
    binding::{GpuBinding, GpuBindingData, GpuSsboAccessType},
    constants::{ConstantValue, GpuConstant},
    structure::GpuStructFieldType,
};

use std::io::Write;

pub struct GlslStructCodeGen<W: Write> {
    writer: std::io::BufWriter<W>,
}

pub struct GlslSetsCodeGen<W: Write> {
    binding_idx: usize,
    set_id_define: String,
    writer: std::io::BufWriter<W>,
}

pub struct GlslConstantsCodeGen<W: Write> {
    writer: std::io::BufWriter<W>,
}

impl<W: Write> GlslStructCodeGen<W> {
    pub fn new(writer: W) -> Self {
        let mut code_gen = Self {
            writer: std::io::BufWriter::new(writer),
        };

        // Write in warning about code gen
        writeln!(
            code_gen.writer,
            "/// WARNING: This file is autogenerated by the build script of"
        )
        .unwrap();
        writeln!(
            code_gen.writer,
            "/// `ard-render-base`. Modifications to this file will be overwritten.\n"
        )
        .unwrap();

        // Write in include guards
        writeln!(code_gen.writer, "#include \"ard_consts.glsl\"\n").unwrap();
        writeln!(code_gen.writer, "#ifndef _TYPES_GLSL").unwrap();
        writeln!(code_gen.writer, "#define _TYPES_GLSL\n").unwrap();
        writeln!(
            code_gen.writer,
            "#extension GL_EXT_shader_explicit_arithmetic_types_int64 : require\n"
        )
        .unwrap();

        code_gen
    }

    fn field_name(ty: &GpuStructFieldType) -> String {
        match ty {
            GpuStructFieldType::Struct(name) => name.clone(),
            GpuStructFieldType::U32 | GpuStructFieldType::USize => "uint".into(),
            GpuStructFieldType::I32 => "int".into(),
            GpuStructFieldType::U16 => "uint16_t".into(),
            GpuStructFieldType::U64 => "uint64_t".into(),
            GpuStructFieldType::F32 => "float".into(),
            GpuStructFieldType::Bool => "uint".into(),
            GpuStructFieldType::UVec2 => "uvec2".into(),
            GpuStructFieldType::UVec4 => "uvec4".into(),
            GpuStructFieldType::IVec2 => "ivec2".into(),
            GpuStructFieldType::Vec2 => "vec2".into(),
            GpuStructFieldType::Vec4 => "vec4".into(),
            GpuStructFieldType::Mat3x4 => "mat3x4".into(),
            GpuStructFieldType::Mat4 => "mat4".into(),
            GpuStructFieldType::Array { ty: inner_ty, .. } => {
                // Find the type stored in the array and put it first
                let inner_ty = Self::find_array_type(inner_ty);

                // Then append the array brackets
                let brackets = Self::find_array_brackets(ty);

                format!("{inner_ty}{brackets}")
            }
        }
    }

    fn find_array_type(ty: &GpuStructFieldType) -> String {
        match ty {
            GpuStructFieldType::Array { ty, .. } => Self::find_array_type(ty),
            _ => Self::field_name(ty),
        }
    }

    fn find_array_brackets(ty: &GpuStructFieldType) -> String {
        let mut brackets = String::default();
        if let GpuStructFieldType::Array { ty, len } = ty {
            brackets = format!("[{len}]{}", Self::find_array_brackets(ty));
        }
        brackets
    }
}

impl<W: Write> Drop for GlslStructCodeGen<W> {
    fn drop(&mut self) {
        writeln!(self.writer, "#endif").unwrap();
    }
}

impl<W: Write> StructCodeGen for GlslStructCodeGen<W> {
    fn begin_struct(&mut self, name: &str) {
        writeln!(self.writer, "struct {name} {{").unwrap();
    }

    fn write_field(&mut self, name: &str, ty: &GpuStructFieldType) {
        let field_name = Self::field_name(ty);
        writeln!(self.writer, "    {field_name} {name};").unwrap();
    }

    fn end_struct(&mut self, _: &str) {
        writeln!(self.writer, "}};\n").unwrap();
    }
}

impl<W: Write> GlslSetsCodeGen<W> {
    pub fn new(writer: W) -> Self {
        let mut code_gen = Self {
            writer: std::io::BufWriter::new(writer),
            set_id_define: String::default(),
            binding_idx: 0,
        };

        // Write in warning about code gen
        writeln!(
            code_gen.writer,
            "/// WARNING: This file is autogenerated by the build script of"
        )
        .unwrap();
        writeln!(
            code_gen.writer,
            "/// `ard-render-base`. Modifications to this file will be overwritten.\n"
        )
        .unwrap();

        // Include constants so they can be used in the sets
        writeln!(code_gen.writer, "#include \"ard_types.glsl\"\n").unwrap();
        writeln!(code_gen.writer, "#include \"ard_consts.glsl\"\n").unwrap();

        // Write in include guards
        writeln!(code_gen.writer, "#ifndef _SETS_GLSL").unwrap();
        writeln!(code_gen.writer, "#define _SETS_GLSL\n").unwrap();

        code_gen
    }
}

impl<W: Write> Drop for GlslSetsCodeGen<W> {
    fn drop(&mut self) {
        writeln!(self.writer, "#endif").unwrap();
    }
}

impl<W: Write> DescriptorSetCodeGen for GlslSetsCodeGen<W> {
    fn begin_set(&mut self, name: &str) {
        self.binding_idx = 0;
        self.set_id_define = format!("ARD_SET_{}", name.to_owned().to_case(Case::UpperSnake));
        writeln!(self.writer, "#ifdef {}\n", self.set_id_define).unwrap();
    }

    fn write_binding(&mut self, name: &str, binding: &GpuBinding) {
        // Write in layout
        match binding.data() {
            GpuBindingData::Ubo(_) | GpuBindingData::Ssbo { .. } => {
                write!(
                    self.writer,
                    "layout(std430, set = {}, binding = {}) ",
                    self.set_id_define, self.binding_idx
                )
                .unwrap();
            }
            GpuBindingData::Texture(_)
            | GpuBindingData::UTexture(_)
            | GpuBindingData::ITexture(_)
            | GpuBindingData::UnboundedTextureArray(_)
            | GpuBindingData::ShadowTextureArray(_)
            | GpuBindingData::CubeMap(_) => {
                write!(
                    self.writer,
                    "layout(set = {}, binding = {}) ",
                    self.set_id_define, self.binding_idx
                )
                .unwrap();
            }
            GpuBindingData::StorageImage { format, .. } => {
                write!(
                    self.writer,
                    "layout(set = {}, binding = {}, {}) ",
                    self.set_id_define,
                    self.binding_idx,
                    format.to_glsl()
                )
                .unwrap();
            }
        }

        // Write in body
        match binding.data() {
            GpuBindingData::Ssbo {
                restrict,
                access,
                inner,
                unbounded_array,
            } => {
                if *restrict {
                    write!(self.writer, "restrict ").unwrap();
                }

                match *access {
                    GpuSsboAccessType::ReadOnly => write!(self.writer, "readonly ").unwrap(),
                    GpuSsboAccessType::WriteOnly => write!(self.writer, "writeonly ").unwrap(),
                    GpuSsboAccessType::ReadWrite => {}
                }

                writeln!(self.writer, "buffer {name} {{").unwrap();

                if let Some(inner) = inner {
                    writeln!(
                        self.writer,
                        "{} {};",
                        GlslStructCodeGen::<W>::field_name(&inner.ty),
                        inner.name,
                    )
                    .unwrap();
                }

                if let Some(field) = unbounded_array {
                    let ty = GpuStructFieldType::Array {
                        ty: Box::new(field.ty.clone()),
                        len: "".into(),
                    };
                    let inner_ty = GlslStructCodeGen::<W>::find_array_type(&field.ty);
                    let brackets = GlslStructCodeGen::<W>::find_array_brackets(&ty);

                    writeln!(self.writer, "    {inner_ty}{brackets} {};", field.name).unwrap();
                }

                writeln!(self.writer, "}};\n").unwrap();
            }
            GpuBindingData::Ubo(field) => {
                writeln!(self.writer, "uniform {} {{", name).unwrap();
                writeln!(
                    self.writer,
                    "    {} {};",
                    GlslStructCodeGen::<W>::field_name(&field.ty),
                    field.name,
                )
                .unwrap();
                writeln!(self.writer, "}};\n").unwrap();
            }
            GpuBindingData::Texture(field_name) => {
                writeln!(self.writer, "uniform sampler2D {field_name};\n").unwrap();
            }
            GpuBindingData::UTexture(field_name) => {
                writeln!(self.writer, "uniform usampler2D {field_name};\n").unwrap();
            }
            GpuBindingData::ITexture(field_name) => {
                writeln!(self.writer, "uniform isampler2D {field_name};\n").unwrap();
            }
            GpuBindingData::CubeMap(field_name) => {
                writeln!(self.writer, "uniform samplerCube {field_name};\n").unwrap();
            }
            GpuBindingData::UnboundedTextureArray(field_name) => {
                writeln!(self.writer, "uniform sampler2D {field_name}[];\n").unwrap();
            }
            GpuBindingData::ShadowTextureArray(field_name) => {
                let count = binding.count();
                writeln!(
                    self.writer,
                    "uniform sampler2DShadow {field_name}[{count}];\n"
                )
                .unwrap();
            }
            GpuBindingData::StorageImage {
                field_name,
                restrict,
                access,
                ..
            } => {
                write!(self.writer, "uniform ").unwrap();

                if *restrict {
                    write!(self.writer, "restrict ").unwrap();
                }

                match *access {
                    GpuSsboAccessType::ReadOnly => write!(self.writer, "readonly ").unwrap(),
                    GpuSsboAccessType::WriteOnly => write!(self.writer, "writeonly ").unwrap(),
                    GpuSsboAccessType::ReadWrite => {}
                }

                writeln!(self.writer, "image2D {field_name};\n").unwrap();
            }
        }

        self.binding_idx += 1;
    }

    fn end_set(&mut self, _: &str) {
        writeln!(self.writer, "#endif\n").unwrap();
    }
}

impl<W: Write> GlslConstantsCodeGen<W> {
    pub fn new(writer: W) -> Self {
        let mut code_gen = Self {
            writer: std::io::BufWriter::new(writer),
        };

        // Write in warning about code gen
        writeln!(
            code_gen.writer,
            "/// WARNING: This file is autogenerated by the build script of"
        )
        .unwrap();
        writeln!(
            code_gen.writer,
            "/// `ard-render-base`. Modifications to this file will be overwritten.\n"
        )
        .unwrap();

        // Write in include guards
        writeln!(code_gen.writer, "#ifndef _CONSTANTS_GLSL").unwrap();
        writeln!(code_gen.writer, "#define _CONSTANTS_GLSL\n").unwrap();

        code_gen
    }
}

impl<W: Write> ConstantsCodeGen for GlslConstantsCodeGen<W> {
    fn write_constant(&mut self, constant: &GpuConstant) {
        let name = constant.name.to_case(Case::UpperSnake);

        let value = match &constant.value {
            ConstantValue::Float(f) => f.to_string(),
            ConstantValue::Int(i) => i.to_string(),
            ConstantValue::UInt(u) => u.to_string(),
            ConstantValue::USize(u) => u.to_string(),
            ConstantValue::Custom(_, c) => c.clone(),
        };

        writeln!(self.writer, "#define {name} {value}").unwrap();
    }
}

impl<W: Write> Drop for GlslConstantsCodeGen<W> {
    fn drop(&mut self) {
        writeln!(self.writer, "\n#endif").unwrap();
    }
}
