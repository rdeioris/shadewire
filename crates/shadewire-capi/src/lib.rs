#![allow(non_camel_case_types)]
//! C API for Shadewire: opaque handles, `shadewire_*` functions, no panic crosses the boundary.
//!
//! It follows the Rust crates and the Python module (`shadewire`) name for name:
//!
//! - `shadewire.mtlx.to_ir`, `.material_names`: `shadewire_mtlx_to_ir`, `shadewire_mtlx_material_names` (a
//!   `shadewire_strings`);
//! - `shadewire.blender.to_ir`, `.material_names`: `shadewire_blender_to_ir` (with `shadewire_ir_options`),
//!   `shadewire_blender_material_names`;
//! - `shadewire.mdl.to_ir`, `.material_names`: `shadewire_mdl_to_ir` (with `shadewire_ir_options` and the MDL search
//!   path), `shadewire_mdl_material_names`; `shadewire.mdl.compile`: `shadewire_compile` with `SHADEWIRE_TARGET_MDL`;
//! - `shadewire.mtlx.from_ir`: `shadewire_ir_to_mtlx` (a `shadewire_document`, with `shadewire_document_text` and
//!   `_warning_count` / `_warning`);
//! - `shadewire_mtlx::list_dependencies` and the resolver argument of `to_ir`: `shadewire_mtlx_list_dependencies` (a
//!   `shadewire_dependencies`) and `shadewire_mtlx_options::resolve` (a `shadewire_resolve_fn` answering through
//!   `shadewire_resolution_set` / `_fail`);
//! - `shadewire.Ir` (`.optimize()`, `.dump()`, `.to_dict()`, `.model`, `.source`, `.structure_hash`, `.warnings`):
//!   `shadewire_ir` (`shadewire_ir_optimize`, `_dump`, `_json`, `_model`, `_source` and `_source_version`,
//!   `_structure_hash`, `_warnings`);
//! - `shadewire.wgsl.compile`, `.glsl`, `.hlsl`, `.msl` and their keyword arguments: `shadewire_compile` with a
//!   `shadewire_target` and `shadewire_compile_options`;
//! - `shadewire.Compiled` (`.language`, `.code`, `.reflection`, `.model_struct`, `.ir`): `shadewire_compiled`
//!   (`shadewire_compiled_language`, `_code`, `_reflection_json`, `_model_struct`, `_ir_dump`);
//! - `IR_FORMAT_VERSION`, `version()`, `ShadewireError`: `SHADEWIRE_IR_FORMAT_VERSION`, `shadewire_version_string`,
//!   `shadewire_error`.

//!
//! Every fallible function takes an `error` out-parameter: it is written only on failure and left untouched on
//! success (callers initialise it to NULL), and may itself be NULL to ignore the error object.
//!
//! Option structs carry `struct_size` and grow only by appending fields after their current end; their tail padding
//! is reserved as explicit `reserved` bytes that callers set to zero and the library ignores on read.
//!
//! Threads: handles are immutable once returned, so any number of threads may call the accessors and
//! `shadewire_compile` on the same handle at once (lazily built texts such as the IR dump
//! are built once, safely). Free a handle exactly once, after every reader is done with it. A `resolve` callback is
//! called only on the thread of the call whose options name it, and only during that call; it needs locking only for
//! `user_data` the host shares between calls running at once.
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/rdeioris/shadewire/main/assets/logo/shadewire-mark.svg",
    html_favicon_url = "https://raw.githubusercontent.com/rdeioris/shadewire/main/assets/logo/png/shadewire-mark-64.png"
)]

mod blender;
mod compiled;
mod dependencies;
mod document;
mod error;
mod error_kind;
mod guard;
mod ir;
mod mdl;
mod names;
mod options;
mod resolver;
mod status;
mod strings;
mod target;
mod threads;
mod version;

pub use blender::{shadewire_blender_material_names, shadewire_blender_to_ir};
pub use compiled::{
    shadewire_compile, shadewire_compiled, shadewire_compiled_code, shadewire_compiled_free,
    shadewire_compiled_ir_dump, shadewire_compiled_language, shadewire_compiled_model_struct,
    shadewire_compiled_reflection_json,
};
pub use dependencies::{
    shadewire_dependencies, shadewire_dependencies_file, shadewire_dependencies_file_count,
    shadewire_dependencies_free, shadewire_dependencies_include, shadewire_dependencies_include_count,
    shadewire_mtlx_list_dependencies,
};
pub use document::{
    shadewire_document, shadewire_document_free, shadewire_document_text, shadewire_document_warning,
    shadewire_document_warning_count, shadewire_ir_to_mtlx,
};
pub use error::{
    shadewire_error, shadewire_error_file, shadewire_error_free, shadewire_error_get_kind, shadewire_error_line,
    shadewire_error_message, shadewire_error_path,
};
pub use error_kind::shadewire_error_kind;
pub use ir::{
    SHADEWIRE_IR_FORMAT_VERSION, shadewire_ir, shadewire_ir_dump, shadewire_ir_free, shadewire_ir_json,
    shadewire_ir_model, shadewire_ir_optimize, shadewire_ir_source, shadewire_ir_source_version,
    shadewire_ir_structure_hash, shadewire_ir_warnings, shadewire_mtlx_to_ir,
};
pub use mdl::{shadewire_mdl_material_names, shadewire_mdl_to_ir};
pub use names::{
    shadewire_mtlx_material_names, shadewire_strings, shadewire_strings_count, shadewire_strings_free,
    shadewire_strings_get,
};
pub use options::{
    shadewire_compile_options, shadewire_compile_options_default, shadewire_ir_options, shadewire_ir_options_default,
    shadewire_mtlx_options, shadewire_mtlx_options_default,
};
pub use resolver::{shadewire_resolution, shadewire_resolution_fail, shadewire_resolution_set, shadewire_resolve_fn};
pub use status::shadewire_status;
pub use target::{
    SHADEWIRE_TARGET_GLSL, SHADEWIRE_TARGET_GLSL_ES, SHADEWIRE_TARGET_HLSL, SHADEWIRE_TARGET_MDL, SHADEWIRE_TARGET_MSL,
    SHADEWIRE_TARGET_WGSL, shadewire_target, shadewire_target_kind,
};
pub use version::{
    SHADEWIRE_VERSION_MAJOR, SHADEWIRE_VERSION_MINOR, SHADEWIRE_VERSION_PATCH, shadewire_version,
    shadewire_version_string,
};
