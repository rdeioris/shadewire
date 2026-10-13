//! MaterialX document reader and graph lowering for Shadewire
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/rdeioris/shadewire/main/assets/logo/shadewire-mark.svg",
    html_favicon_url = "https://raw.githubusercontent.com/rdeioris/shadewire/main/assets/logo/png/shadewire-mark-64.png"
)]

pub mod document;
pub mod elaborate;
pub mod element;
pub mod include;
pub mod index;
pub mod library;
pub mod parse;
pub mod resolve_nodedef;
pub mod surface;
pub mod to_ir;
pub mod upgrade;
pub mod version;

pub use document::Document;
pub use elaborate::{ElaborateOptions, elaborate};
pub use element::Element;
pub use include::{ResolveFn, Resolved, Resolver};
pub use index::Index;
pub use library::Library;
pub use resolve_nodedef::resolve_nodedef;
pub use to_ir::{MaterialIr, MtlxOptions, material_names, to_ir};
