//! NVIDIA MDL for Shadewire: MDL modules read into the IR, and the IR written as a self-contained MDL module.
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/rdeioris/shadewire/main/assets/logo/shadewire-mark.svg",
    html_favicon_url = "https://raw.githubusercontent.com/rdeioris/shadewire/main/assets/logo/png/shadewire-mark-64.png"
)]

pub mod ast;
pub mod parse;
pub mod program;
pub mod types;
pub mod value;

pub use program::Resolver;
mod base;
pub mod eval;
pub mod intrinsics;
pub mod material;

pub use material::{material_names, to_ir};
/// The options and result types of [`to_ir`], re-exported so an MDL user needs one crate.
pub use shadewire_nodes::{IrOptions, MaterialIr};
