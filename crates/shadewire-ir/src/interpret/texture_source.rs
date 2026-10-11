use crate::TextureDesc;

/// Samples textures for the interpreter. Address modes, filtering and the V flip
/// belong to the implementation: the interpreter passes MaterialX uv unchanged.
pub trait TextureSource {
    fn sample(&self, texture: &TextureDesc, uv: [f32; 2]) -> Option<[f32; 4]>;
}

/// A texture source with no images: every sample falls back to its default.
pub struct NoTextures;

impl TextureSource for NoTextures {
    fn sample(&self, _texture: &TextureDesc, _uv: [f32; 2]) -> Option<[f32; 4]> {
        None
    }
}
