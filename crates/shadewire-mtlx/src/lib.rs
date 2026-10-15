//! MaterialX document reader for Shadewire: parse, includes, upgrade, then lowering through `shadewire-nodes`; and the
//! writer back to MaterialX (`from_ir`).
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/rdeioris/shadewire/main/assets/logo/shadewire-mark.svg",
    html_favicon_url = "https://raw.githubusercontent.com/rdeioris/shadewire/main/assets/logo/png/shadewire-mark-64.png"
)]

pub mod dependencies;
pub mod include;
pub mod to_ir;
pub mod upgrade;
pub mod write;

pub use dependencies::{Dependencies, list_dependencies};
pub use include::{ResolveFn, Resolved, Resolver};
/// The options type of [`to_ir`], re-exported so a MaterialX user needs one crate.
pub use shadewire_nodes::{IrOptions, MaterialIr};
pub use to_ir::{material_names, to_ir};
pub use write::{WriteOptions, Written, from_ir};
