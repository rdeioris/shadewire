#![allow(non_camel_case_types, clippy::missing_safety_doc)]
//! C API for Shadewire: opaque handles, `shadewire_*` functions, no panic crosses the boundary.

mod error;
mod guard;
mod status;
mod strings;
mod version;

pub use error::{CapiError, shadewire_error};
pub use guard::guarded;
pub use status::shadewire_status;
pub use strings::{ArgError, c_string, opt_cstr, out_str, text_arg};
pub use version::{shadewire_version, shadewire_version_string};
