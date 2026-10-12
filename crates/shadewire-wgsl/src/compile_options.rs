/// Back-end knobs: how the IR becomes WGSL and reflection. `Default` is what most hosts want.
#[derive(Clone, Debug, PartialEq)]
pub struct CompileOptions {
    /// Bind group of the generated uniform block, textures and samplers.
    pub group: u32,
    /// Binding of the first generated resource.
    pub first_binding: u32,
    /// Keep `first_binding` for the uniform block even when the material has no uniforms: textures then start at
    /// `first_binding + 1` and `uniform_binding` stays `None`. For hosts that bind their own buffer there.
    pub reserve_uniform_binding: bool,
    /// Flip V when sampling: hosts upload images top row first.
    pub flip_v: bool,
    /// Run the optimizer before reflection and emission.
    pub optimize: bool,
}

impl Default for CompileOptions {
    fn default() -> CompileOptions {
        CompileOptions { group: 2, first_binding: 0, reserve_uniform_binding: false, flip_v: true, optimize: true }
    }
}
