use std::path::PathBuf;

use thiserror::Error;

/// Errors from building a Shadewire material for Bevy. New variants may be added.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ShadewireError {
    #[error(transparent)]
    Shadewire(#[from] shadewire::Error),
    #[error("cannot read {}: {source}", path.display())]
    Read { path: PathBuf, source: std::io::Error },
    #[error("cannot read texture {}: {source}", path.display())]
    TextureRead { path: PathBuf, source: std::io::Error },
    #[error("cannot decode texture {}: {message}", path.display())]
    TextureDecode { path: PathBuf, message: String },
    #[error("uniform block is {size} bytes; the Bevy extension holds at most {limit} bytes")]
    UniformsTooLarge { size: u32, limit: u32 },
    #[error("material uses {count} textures; the Bevy extension holds at most {limit}")]
    TooManyTextures { count: usize, limit: usize },
    #[error("host functions Bevy does not provide: {}", names.join(", "))]
    UnsupportedHost { names: Vec<String> },
    #[error("shading model `{0}` has no Bevy mapping")]
    UnsupportedModel(String),
    #[error("{0}; compile with shadewire_bevy::compile_options()")]
    Bindings(String),
}
