use crate::{ClosureGraph, NodeId, Type};

/// The models a host passes to the front end to have every material opened into its closure tree (named models
/// through their implementation graphs) and lit by MaterialX's reference lobes. `surface_unlit` has no lobes and
/// stays unshaded.
pub const LOBE_MODELS: [&str; 2] = ["surface", "surface_unlit"];

#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceField {
    pub name: String,
    pub ty: Type,
    pub node: NodeId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Surface {
    pub model: String,
    pub fields: Vec<SurfaceField>,
    /// The closure tree of a `surface` model, whose fields are the tree's values; `None` for every other model.
    pub closure: Option<ClosureGraph>,
}

/// The WGSL struct name of a shading model: split on `_`, capitalise each part, join.
pub fn model_struct_name(model: &str) -> String {
    let mut out = String::new();
    for part in model.split('_') {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::model_struct_name;

    #[test]
    fn struct_names_of_the_phase_one_models() {
        assert_eq!(model_struct_name("open_pbr_surface"), "OpenPbrSurface");
        assert_eq!(model_struct_name("standard_surface"), "StandardSurface");
        assert_eq!(model_struct_name("gltf_pbr"), "GltfPbr");
        assert_eq!(model_struct_name("surface_unlit"), "SurfaceUnlit");
    }
}
