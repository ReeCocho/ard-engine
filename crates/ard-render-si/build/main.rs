use std::env;

use std::{collections::HashMap, path::PathBuf};

use binding::{GpuSet, GpuSetBuilder};
use code_gen::{
    glsl::{GlslConstantsCodeGen, GlslSetsCodeGen},
    rust::{RustConstantsCodeGen, RustSetsCodeGen},
};
use constants::GpuConstant;
use structure::{GpuStruct, GpuStructBuilder};

use crate::code_gen::{glsl::GlslStructCodeGen, rust::RustStructCodeGen};

mod binding;
mod code_gen;
mod constants;
mod structure;

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();

    std::fs::create_dir_all(PathBuf::from(&out_dir).join("glsl/")).unwrap();

    gen_types(&out_dir, "./defs/types.ron");
    gen_consts(&out_dir, "./defs/constants.ron");
    gen_bindings(&out_dir, "./defs/bindings.ron");

    println!("cargo:rerun-if-changed=defs/bindings.ron");
    println!("cargo:rerun-if-changed=defs/constants.ron");
    println!("cargo:rerun-if-changed=defs/types.ron");
}

fn gen_types(output_dir: impl Into<PathBuf>, types_file: impl Into<PathBuf>) {
    let output_dir = output_dir.into();
    let structs_file = types_file.into();

    let mut structs = Vec::<GpuStructBuilder>::default();

    // Read in the structures definition
    let data = match std::fs::read_to_string(&structs_file) {
        Ok(data) => data,
        Err(err) => {
            println!("cargo:warning=Unable to read file `{structs_file:?}`. Error: {err:?}");
            return;
        }
    };

    // Parse the file
    let struct_defs = match ron::de::from_str::<Vec<GpuStruct>>(&data) {
        Ok(defs) => defs,
        Err(err) => {
            println!("cargo:warning=Unable to parse `{structs_file:?}`. Error: {err:?}");
            return;
        }
    };

    // Build structures
    for s in struct_defs {
        let mut builder = GpuStructBuilder::new(s.name().into());
        builder.no_mangle(s.no_mangle());
        for field in s.into_fields() {
            builder.add(field);
        }

        structs.push(builder);
    }

    // Perform code gen
    let mut rust_code_gen = RustStructCodeGen::new(
        std::fs::OpenOptions::new()
            .write(true)
            .append(false)
            .create(true)
            .truncate(true)
            .open(output_dir.clone().join("gpu_types.rs"))
            .unwrap(),
    );

    let glsl_path = output_dir.join("glsl/ard_types.glsl");

    let mut glsl_code_gen = GlslStructCodeGen::new(
        std::fs::OpenOptions::new()
            .write(true)
            .append(false)
            .create(true)
            .truncate(true)
            .open(glsl_path)
            .unwrap(),
    );

    for s in structs.into_iter() {
        let s = s.build().unwrap();
        s.gen(&mut rust_code_gen);
        s.gen(&mut glsl_code_gen);
    }
}

fn gen_consts(output_dir: impl Into<PathBuf>, consts_file: impl Into<PathBuf>) {
    let output_dir = output_dir.into();
    let consts_file = consts_file.into();

    // Read in the constants definition
    let data = match std::fs::read_to_string(&consts_file) {
        Ok(data) => data,
        Err(err) => {
            println!("cargo:warning=Unable to read file `{consts_file:?}`. Error: {err:?}");
            return;
        }
    };

    // Parse the file
    let consts = match ron::de::from_str::<Vec<GpuConstant>>(&data) {
        Ok(defs) => defs,
        Err(err) => {
            println!("cargo:warning=Unable to parse `{consts_file:?}`. Error: {err:?}");
            return;
        }
    };

    // Perform code gen
    let mut rust_code_gen = RustConstantsCodeGen::new(
        std::fs::OpenOptions::new()
            .write(true)
            .append(false)
            .create(true)
            .truncate(true)
            .open(output_dir.clone().join("gpu_consts.rs"))
            .unwrap(),
    );

    let glsl_path = output_dir.join("glsl/ard_consts.glsl");

    let mut glsl_code_gen = GlslConstantsCodeGen::new(
        std::fs::OpenOptions::new()
            .write(true)
            .append(false)
            .create(true)
            .truncate(true)
            .open(glsl_path)
            .unwrap(),
    );

    for c in consts.into_iter() {
        c.gen(&mut rust_code_gen);
        c.gen(&mut glsl_code_gen);
    }
}

fn gen_bindings(output_dir: impl Into<PathBuf>, bindings_file: impl Into<PathBuf>) {
    let output_dir = output_dir.into();
    let bindings_file = bindings_file.into();

    // Mapping from GPU set names to their associated builders
    let mut sets = HashMap::<String, GpuSetBuilder>::default();

    let data = match std::fs::read_to_string(&bindings_file) {
        Ok(data) => data,
        Err(err) => {
            println!("cargo:warning=Unable to read file `{bindings_file:?}`. Error: {err:?}");
            return;
        }
    };

    let set_defs = match ron::de::from_str::<Vec<GpuSet>>(&data) {
        Ok(defs) => defs,
        Err(err) => {
            println!("cargo:warning=Unable to parse `{bindings_file:?}`. Error: {err:?}");
            return;
        }
    };

    for set in set_defs {
        let entry = sets
            .entry(set.name().into())
            .or_insert_with(|| GpuSetBuilder::new(set.name().into()));

        for binding in set.into_bindings() {
            entry.add(binding);
        }
    }

    let mut rust_code_gen = RustSetsCodeGen::new(
        std::fs::OpenOptions::new()
            .write(true)
            .append(false)
            .create(true)
            .truncate(true)
            .open(output_dir.clone().join("gpu_bindings.rs"))
            .unwrap(),
    );

    let glsl_path = output_dir.clone().join("glsl/ard_bindings.glsl");

    let mut glsl_code_gen = GlslSetsCodeGen::new(
        std::fs::OpenOptions::new()
            .write(true)
            .append(false)
            .create(true)
            .truncate(true)
            .open(glsl_path)
            .unwrap(),
    );

    for s in sets.into_values() {
        let s = s.build();
        s.gen(&mut rust_code_gen);
        s.gen(&mut glsl_code_gen);
    }
}
