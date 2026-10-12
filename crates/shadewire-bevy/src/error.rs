use thiserror::Error;

/// Errors from building a Shadewire material for Bevy.
#[derive(Debug, Error)]
pub enum ShadewireError {
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
