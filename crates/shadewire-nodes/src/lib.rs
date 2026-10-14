//! The node library of Shadewire: every node a material graph can use, how each is defined, and how an instance
//! of it becomes IR. Front ends (`shadewire-mtlx`, `shadewire-blender`) walk their own graphs and ask this crate
//! for nodes.
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/rdeioris/shadewire/main/assets/logo/shadewire-mark.svg",
    html_favicon_url = "https://raw.githubusercontent.com/rdeioris/shadewire/main/assets/logo/png/shadewire-mark-64.png"
)]

pub mod document;
pub mod elaborate;
pub mod element;
pub mod index;
pub mod library;
pub mod options;
pub mod parse;
pub mod resolve_nodedef;
pub mod surface;
pub mod synthetic;
pub mod version;

pub use document::Document;
pub use elaborate::{Arg, Elaborator, elaborate};
pub use element::Element;
pub use index::Index;
pub use library::Library;
pub use options::{DEFAULT_MODELS, IrOptions};
pub use resolve_nodedef::resolve_nodedef;
pub use surface::ShaderNode;
pub use synthetic::Synthetic;
