//! MaterialX document reader and graph lowering for Shadewire

pub mod compile_mtlx;
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

pub use compile_mtlx::compile_mtlx;
pub use document::Document;
pub use elaborate::{ElaborateOptions, elaborate};
pub use element::Element;
pub use index::Index;
pub use library::Library;
pub use resolve_nodedef::resolve_nodedef;
pub use to_ir::{MaterialIr, MtlxOptions, to_ir};
