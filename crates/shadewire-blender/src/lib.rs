//! Blender material graphs as Shadewire IR: the `shadewire-blender-graph` document the Blender extension writes,
//! parsed, flattened and lowered through `shadewire-nodes`.
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/rdeioris/shadewire/main/assets/logo/shadewire-mark.svg",
    html_favicon_url = "https://raw.githubusercontent.com/rdeioris/shadewire/main/assets/logo/png/shadewire-mark-64.png"
)]

pub mod document;
pub mod graph;

pub use document::material_names;
/// The options and result types of [`to_ir`], re-exported so a Blender user needs one crate.
pub use shadewire_nodes::{IrOptions, MaterialIr};
