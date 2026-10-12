/// Back-end knobs: how the IR becomes WGSL and reflection. `Default` is what most hosts want.
#[derive(Clone, Debug, PartialEq)]
pub struct CompileOptions {
    /// Bind group of the generated uniform block, textures and samplers.
    pub group: u32,
    /// Binding of the first generated resource.
    pub first_binding: u32,
    /// Flip V when sampling: hosts upload images top row first.
    pub flip_v: bool,
    /// Run the optimizer before reflection and emission.
    pub optimize: bool,
}

impl Default for CompileOptions {
    fn default() -> CompileOptions {
        CompileOptions { group: 2, first_binding: 0, flip_v: true, optimize: true }
    }
}
