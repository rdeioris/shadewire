/// How the material's opacity reaches Godot. `render_mode` cannot depend on uniforms, so this is
/// decided at compile time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Transparency {
    /// Opaque when the opacity is the constant 1, alpha scissor when a glTF `alpha_mode` is the
    /// constant MASK (opaque for the constant OPAQUE), alpha blending otherwise.
    #[default]
    Auto,
    /// `ALPHA` is not written.
    Opaque,
    /// `ALPHA` with `ALPHA_SCISSOR_THRESHOLD` (glTF `alpha_cutoff`, else 0.5).
    Scissor,
    /// `ALPHA` with `ALPHA_HASH_SCALE`: sorts correctly and casts shadows, for foliage.
    Hash,
    /// `ALPHA` with `blend_mix`.
    Blend,
}

/// Back-end knobs. `Default` is what most hosts want.
#[derive(Clone, Debug, PartialEq)]
pub struct GdshaderOptions {
    /// Run the optimizer before reflection and emission.
    pub optimize: bool,
    pub transparency: Transparency,
    /// Multiplies `EMISSION`, for projects with physical light units; emission otherwise passes 1:1.
    pub emission_scale: f32,
}

impl Default for GdshaderOptions {
    fn default() -> GdshaderOptions {
        GdshaderOptions { optimize: true, transparency: Transparency::Auto, emission_scale: 1.0 }
    }
}
