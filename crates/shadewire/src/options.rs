/// Knobs of a compilation. `Default` is what most hosts want.
#[derive(Clone, Debug, PartialEq)]
pub struct CompileOptions {
    /// Surface models the host accepts; a material of another model is `NoTranslation`.
    pub models: Vec<String>,
    /// Bind group of the generated uniform block, textures and samplers.
    pub group: u32,
    /// Binding of the first generated resource.
    pub first_binding: u32,
    pub working_colorspace: String,
    /// Authored surface values and top-level interface values become uniforms.
    pub values_as_uniforms: bool,
    /// Flip V when sampling: hosts upload images top row first.
    pub flip_v: bool,
    /// Run the optimizer before reflection and emission.
    pub optimize: bool,
}

impl Default for CompileOptions {
    fn default() -> CompileOptions {
        CompileOptions {
            models: ["open_pbr_surface", "standard_surface", "gltf_pbr", "surface_unlit"]
                .iter()
                .map(|m| m.to_string())
                .collect(),
            group: 2,
            first_binding: 0,
            working_colorspace: "lin_rec709".to_string(),
            values_as_uniforms: true,
            flip_v: true,
            optimize: true,
        }
    }
}
