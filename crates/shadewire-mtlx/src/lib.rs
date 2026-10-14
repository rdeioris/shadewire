//! MaterialX document reader for Shadewire: parse, includes, upgrade, then lowering through `shadewire-nodes`.
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/rdeioris/shadewire/main/assets/logo/shadewire-mark.svg",
    html_favicon_url = "https://raw.githubusercontent.com/rdeioris/shadewire/main/assets/logo/png/shadewire-mark-64.png"
)]

pub mod dependencies;
pub mod include;
pub mod to_ir;
pub mod upgrade;

pub use dependencies::{Dependencies, list_dependencies};
/// The options type of [`to_ir`], re-exported so a MaterialX user needs one crate.
pub use shadewire_nodes::IrOptions;
pub use include::{ResolveFn, Resolved, Resolver};
pub use to_ir::{MaterialIr, material_names, to_ir};
