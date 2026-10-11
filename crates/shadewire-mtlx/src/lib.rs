//! MaterialX document reader and graph lowering for Shadewire

pub mod document;
pub mod element;
pub mod include;
pub mod parse;
pub mod version;

pub use element::Element;
pub use document::Document;
pub mod index;
pub mod library;

pub use index::Index;
pub use library::Library;
