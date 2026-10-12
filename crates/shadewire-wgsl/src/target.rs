//! Output languages for `translate`. Shadewire's own types, so callers never depend on naga.

/// HLSL shader model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HlslShaderModel {
    Sm5_0,
    Sm5_1,
    Sm6_0,
    Sm6_1,
    Sm6_2,
    Sm6_3,
    Sm6_4,
    Sm6_5,
    Sm6_6,
    Sm6_7,
    Sm6_8,
    Sm6_9,
}

/// GLSL dialect and version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlslVersion {
    /// Desktop GLSL: 330, 400, 410, 420, 430 or 450.
    Desktop(u16),
    /// GLSL ES: 300 or 310.
    Es(u16),
    /// GLSL ES 300 for WebGL 2.
    WebGl2,
}

/// Metal Shading Language version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MslVersion {
    pub major: u8,
    pub minor: u8,
}

/// A language `translate` writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Wgsl,
    Hlsl(HlslShaderModel),
    Glsl(GlslVersion),
    Msl(MslVersion),
    SpirV,
}
