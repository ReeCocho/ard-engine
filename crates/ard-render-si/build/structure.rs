use std::collections::{HashMap, HashSet};

pub use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::code_gen::StructCodeGen;

/// Describes a data structure used by the GPU and CPU.
#[derive(Debug, Serialize, Deserialize)]
pub struct GpuStruct {
    name: String,
    no_mangle: bool,
    fields: Vec<GpuStructField>,
}

/// Describes a field stored in a data structure shared between the GPU and CPU.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GpuStructField {
    /// Name of the field.
    pub name: String,
    /// Type of data stored in the field.
    pub ty: GpuStructFieldType,
}

/// Type of data stored in a data structure field on the GPU.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub enum GpuStructFieldType {
    Struct(String),
    USize,
    U32,
    I32,
    U64,
    F32,
    Bool,
    UVec2,
    IVec2,
    Vec2,
    Vec4,
    Mat4,
    Array {
        ty: Box<GpuStructFieldType>,
        /// NOTE: The length of the array is a string and not a `usize` to support global constants
        /// for array sizes.
        len: String,
    },
}

#[derive(Default)]
pub struct GpuStructBuilder {
    name: String,
    no_mangle: bool,
    fields: Vec<GpuStructField>,
}

#[derive(Debug, Error)]
pub enum GpuStructBuildError {
    #[error("field `{name}` expected type `{expected:?}` but received `{received:?}`")]
    MismatchingType {
        name: String,
        expected: GpuStructFieldType,
        received: GpuStructFieldType,
    },
}

impl GpuStruct {
    pub fn gen(&self, code_gen: &mut impl StructCodeGen) {
        code_gen.begin_struct(&self.name);
        for field in &self.fields {
            code_gen.write_field(&field.name, &field.ty);
        }
        code_gen.end_struct(&self.name);
    }

    pub fn no_mangle(&self) -> bool {
        self.no_mangle
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn into_fields(self) -> Vec<GpuStructField> {
        self.fields
    }
}

impl GpuStructFieldType {
    pub fn size(&self) -> usize {
        match self {
            GpuStructFieldType::Struct(_) => usize::MAX,
            GpuStructFieldType::U64 => 8,
            GpuStructFieldType::USize => 4,
            GpuStructFieldType::U32 => 4,
            GpuStructFieldType::I32 => 4,
            GpuStructFieldType::F32 => 4,
            GpuStructFieldType::Bool => 4,
            GpuStructFieldType::UVec2 => 8,
            GpuStructFieldType::IVec2 => 8,
            GpuStructFieldType::Vec2 => 8,
            GpuStructFieldType::Vec4 => 16,
            GpuStructFieldType::Mat4 => 64,
            // NOTE: We say arrays are of length 16 because std140 layout forces all arrays
            // of any type to be 16 byte aligned
            GpuStructFieldType::Array { .. } => 16,
        }
    }
}

impl GpuStructBuilder {
    pub fn new(name: String) -> Self {
        Self {
            name,
            no_mangle: false,
            fields: Vec::default(),
        }
    }

    pub fn add(&mut self, field: GpuStructField) {
        self.fields.push(field);
    }

    pub fn no_mangle(&mut self, no_mangle: bool) {
        self.no_mangle |= no_mangle;
    }

    pub fn build(mut self) -> Result<GpuStruct, GpuStructBuildError> {
        let mut fields = HashMap::<String, GpuStructField>::default();

        // Loop over every field that was registered
        for new_field in &self.fields {
            // See if the field already exists in the map
            if let Some(field) = fields.get(&new_field.name) {
                // If it does, ensure if has the same type
                if field.ty != new_field.ty {
                    return Err(GpuStructBuildError::MismatchingType {
                        name: field.name.clone(),
                        expected: field.ty.clone(),
                        received: new_field.ty.clone(),
                    });
                }
            }

            // Insert the field into the list
            fields.insert(new_field.name.clone(), new_field.clone());
        }

        // If we were requested to not mangle the structure, the take fields as they came. Just
        // prune duplicates.
        let fields = if self.no_mangle {
            let mut visited = HashSet::<String>::default();
            self.fields
                .retain_mut(|elem| visited.insert(elem.name.clone()));
            self.fields
        } else {
            // First, sort the fields by name. We do this so that the order of struct fields is
            // deterministic for the same set of fields.
            //
            // Then sort fields for efficient space usage (biggest to smallest)
            let mut fields: Vec<_> = fields.into_values().collect();
            fields.sort_by_key(|elem| elem.name.clone());
            fields.sort_by_key(|elem| -(elem.ty.size() as isize));
            fields
        };

        Ok(GpuStruct {
            name: self.name,
            no_mangle: self.no_mangle,
            fields,
        })
    }
}
