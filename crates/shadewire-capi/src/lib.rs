#![allow(non_camel_case_types)]
//! C API for Shadewire: opaque handles, `shadewire_*` functions, no panic crosses the boundary.
//!
//! Every fallible function takes an `error` out-parameter: it is written only on failure and left untouched on
//! success (callers initialise it to NULL), and may itself be NULL to ignore the error object.
//!
//! Option structs carry `struct_size` and grow only by appending fields after their current end; their tail padding
//! is reserved as explicit `reserved` bytes that callers set to zero and the library ignores on read.
//!
//! Threads: handles are immutable once returned, so any number of threads may call the accessors and
//! `shadewire_wgsl_compile` / `shadewire_translate` on the same handle at once (lazily built texts such as the IR dump
//! are built once, safely). Free a handle exactly once, after every reader is done with it.

mod blob;
mod compiled;
mod error;
mod error_kind;
mod guard;
mod ir;
mod options;
mod status;
mod strings;
mod target;
mod threads;
mod translate;
mod version;

pub use blob::{shadewire_blob, shadewire_blob_data, shadewire_blob_free, shadewire_blob_is_text};
pub use compiled::{
    shadewire_compile_mtlx, shadewire_compiled, shadewire_compiled_free, shadewire_compiled_ir_dump,
    shadewire_compiled_reflection_json, shadewire_compiled_struct_name, shadewire_compiled_wgsl,
    shadewire_wgsl_compile,
};
pub use error::{
    shadewire_error, shadewire_error_file, shadewire_error_free, shadewire_error_get_kind, shadewire_error_line,
    shadewire_error_message, shadewire_error_path,
};
pub use error_kind::shadewire_error_kind;
pub use ir::{shadewire_ir, shadewire_ir_dump, shadewire_ir_free, shadewire_ir_model, shadewire_mtlx_to_ir};
pub use options::{
    shadewire_compile_options, shadewire_compile_options_default, shadewire_mtlx_options,
    shadewire_mtlx_options_default,
};
pub use status::shadewire_status;
pub use target::{
    SHADEWIRE_TARGET_GLSL, SHADEWIRE_TARGET_GLSL_ES, SHADEWIRE_TARGET_HLSL, SHADEWIRE_TARGET_MSL,
    SHADEWIRE_TARGET_SPIRV, SHADEWIRE_TARGET_WEBGL2, SHADEWIRE_TARGET_WGSL, shadewire_target, shadewire_target_kind,
};
pub use translate::shadewire_translate;
pub use version::{shadewire_version, shadewire_version_string};
