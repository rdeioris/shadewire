/// Back-end knobs every back end shares: how the IR becomes code and reflection. `Default` is what most hosts
/// want.
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
    /// Sample textures with `textureSample` and differentiate with `dpdx`/`dpdy`, which only a fragment stage
    /// may call. Off, plain samples read mip level 0 (`textureSampleLevel`) and derivatives are zero, so the
    /// module also runs in a compute stage (a ray tracer's shading).
    pub implicit_derivatives: bool,
    /// Carry the screen-space derivatives of whatever a texture coordinate depends on through the graph, from the
    /// host's derivatives of the context reads (`shadewire_host_<read>_dx` / `_dy`, listed in
    /// `context.derivatives`), sample with explicit gradients, and answer `dpdx` / `dpdy` with the carried
    /// derivatives. Overrides `implicit_derivatives`. For hosts whose pixels need not share a triangle with their
    /// 2 × 2 quad, as a visibility buffer's full-screen shading.
    pub host_derivatives: bool,
}

impl Default for CompileOptions {
    fn default() -> CompileOptions {
        CompileOptions {
            group: 2,
            first_binding: 0,
            reserve_uniform_binding: false,
            flip_v: true,
            optimize: true,
            implicit_derivatives: true,
            host_derivatives: false,
        }
    }
}
