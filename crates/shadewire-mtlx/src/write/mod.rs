//! The IR as a MaterialX document.

pub mod catalog;
pub mod elements;
pub mod emit;
pub mod names;
pub mod patterns;
pub mod plan;
pub mod reach;
pub mod values;
pub mod xml;

/// How `from_ir` writes a document.
#[derive(Clone, Debug, PartialEq)]
pub struct WriteOptions {
    /// Write every library node expanded into the nodes it was built from, instead of as one node.
    pub inline: bool,
    /// Embed the declarations of Shadewire's own nodedefs in documents that use them.
    pub embed_shadewire_defs: bool,
    /// The material's name; the nodegraph and shader names derive from it.
    pub name: String,
}

impl Default for WriteOptions {
    fn default() -> WriteOptions {
        WriteOptions { inline: false, embed_shadewire_defs: true, name: "material".to_string() }
    }
}
