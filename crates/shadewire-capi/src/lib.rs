#![allow(non_camel_case_types, clippy::missing_safety_doc)]
//! C API for Shadewire: opaque handles, `shadewire_*` functions, no panic crosses the boundary.
//!
//! Every fallible function takes an `error` out-parameter: it is written only on failure and left untouched on
//! success (callers initialise it to NULL), and may itself be NULL to ignore the error object.

mod error;
mod error_kind;
mod guard;
mod options;
mod status;
mod strings;
mod target;
mod version;

pub use error::{CapiError, shadewire_error};
pub use error_kind::shadewire_error_kind;
pub use options::{
    shadewire_compile_options, shadewire_compile_options_default, shadewire_mtlx_options,
    shadewire_mtlx_options_default,
};
pub use status::shadewire_status;
pub use strings::{ArgError, c_string, opt_cstr, out_str, text_arg};
pub use target::{
    SHADEWIRE_TARGET_GLSL, SHADEWIRE_TARGET_GLSL_ES, SHADEWIRE_TARGET_HLSL, SHADEWIRE_TARGET_MSL,
    SHADEWIRE_TARGET_SPIRV, SHADEWIRE_TARGET_WEBGL2, SHADEWIRE_TARGET_WGSL, shadewire_target, shadewire_target_kind,
    to_target,
};
pub use version::{shadewire_version, shadewire_version_string};
