use thiserror::Error;

/// Errors from building a Shadewire material for Bevy.
#[derive(Debug, Error)]
pub enum ShadewireError {
    #[error("uniform block is {size} bytes; the Bevy extension holds at most {limit} bytes")]
    UniformsTooLarge { size: u32, limit: u32 },
    #[error("material uses {count} textures; the Bevy extension holds at most {limit}")]
    TooManyTextures { count: usize, limit: usize },
}
