//! Shader definitions for `gearbox`, written in Rust and compiled to WGSL by
//! `magician_rust::build` (see `build.rs`) via the `magician-vgpu` shader DSL
//! (the `#[shader]`, `ShaderGroup`, `ShaderLayout`, and `BindableObject` macros).
//! Each module's generated WGSL and `SHADER_*` source constants are consumed by
//! the corresponding `Material`/`Mesh` implementation in the main `gearbox` crate
//! to build render pipelines.

pub mod basic_material;
pub mod basic_vertex;
pub mod common;
pub mod simple_textured;
