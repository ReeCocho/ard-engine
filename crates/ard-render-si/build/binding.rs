use crate::{code_gen::DescriptorSetCodeGen, structure::GpuStructField};
use ard_pal::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuSet {
    name: String,
    bindings: Vec<GpuBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuBinding {
    name: String,
    stage: ShaderStage,
    count: String,
    data: GpuBindingData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GpuBindingData {
    Ssbo {
        restrict: bool,
        access: GpuSsboAccessType,
        inner: Option<GpuStructField>,
        unbounded_array: Option<GpuStructField>,
    },
    Ubo(GpuStructField),
    Texture(String),
    UTexture(String),
    ITexture(String),
    CubeMap(String),
    UnboundedTextureArray(String),
    ShadowTextureArray(String),
    StorageImage {
        field_name: String,
        restrict: bool,
        access: GpuSsboAccessType,
        format: GpuStorageImageFormat,
    },
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GpuSsboAccessType {
    ReadOnly,
    WriteOnly,
    ReadWrite,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GpuStorageImageFormat {
    R8,
    R16F,
    R32F,
    Rgba16F,
    Rgba8UNorm,
    Rg8UNorm,
}

pub struct GpuSetBuilder {
    name: String,
    bindings: Vec<GpuBinding>,
}

impl GpuSet {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn into_bindings(self) -> Vec<GpuBinding> {
        self.bindings
    }

    pub fn gen(&self, code_gen: &mut impl DescriptorSetCodeGen) {
        code_gen.begin_set(&self.name);
        for binding in &self.bindings {
            code_gen.write_binding(&binding.name, binding);
        }
        code_gen.end_set(&self.name);
    }
}

impl GpuBinding {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn stage(&self) -> ShaderStage {
        self.stage
    }

    pub fn count(&self) -> &str {
        &self.count
    }

    pub fn data(&self) -> &GpuBindingData {
        &self.data
    }
}

impl GpuStorageImageFormat {
    pub fn to_glsl(self) -> &'static str {
        match self {
            GpuStorageImageFormat::R8 => "r8",
            GpuStorageImageFormat::R16F => "r16f",
            GpuStorageImageFormat::R32F => "r32f",
            GpuStorageImageFormat::Rgba16F => "rgba16f",
            GpuStorageImageFormat::Rgba8UNorm => "rgba8",
            GpuStorageImageFormat::Rg8UNorm => "rg8",
        }
    }
}

impl GpuSsboAccessType {
    pub fn to_pal_access_type(self) -> AccessType {
        match self {
            GpuSsboAccessType::ReadOnly => AccessType::Read,
            GpuSsboAccessType::WriteOnly => AccessType::ReadWrite,
            GpuSsboAccessType::ReadWrite => AccessType::ReadWrite,
        }
    }
}

impl GpuSetBuilder {
    pub fn new(name: String) -> Self {
        Self {
            name,
            bindings: Vec::default(),
        }
    }

    pub fn add(&mut self, new_binding: GpuBinding) {
        for binding in &self.bindings {
            if binding.name() == new_binding.name() {
                panic!(
                    "Binding `{}` of set `{}` defined multiple times.",
                    new_binding.name(),
                    self.name
                );
            }
        }

        self.bindings.push(new_binding);
    }

    pub fn build(self) -> GpuSet {
        GpuSet {
            name: self.name,
            bindings: self.bindings,
        }
    }
}
