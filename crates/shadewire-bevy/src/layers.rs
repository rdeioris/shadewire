//! The base `StandardMaterial`'s clearcoat and specular transmission, turned on when the graph drives them.
//!
//! Bevy compiles clearcoat and specular transmission into a material's shaders only when the base
//! material's own factors are positive: `StandardMaterialKey::CLEARCOAT` is `clearcoat > 0.0` and
//! `SPECULAR_TRANSMISSION` is `specular_transmission > 0.0` (bevy_pbr 0.20.0 src/pbr_material.rs:1317,
//! :976), and the transmission one also moves the mesh into the transmissive phase. The generated
//! fragment overwrites both factors with the graph's values, so a base with them at 0 would silently
//! drop a coat or transmission the graph sets. When the graph's field is not known to be 0 and the base
//! factor is 0, the base gets [`ENABLED`] instead.
//!
//! "Known to be 0": a constant 0, or a uniform whose built value is 0 (the extension's uniform block is
//! packed from those values). A uniform later raised from 0 through `ShadewireExtension::params` needs
//! the app to set the base factor itself.

use bevy::pbr::StandardMaterial;
use shadewire::{Reflection, Value};

/// The base factor that turns a layer's shader path on; the generated shader replaces it.
pub const ENABLED: f32 = 1e-3;

/// The model's coat and transmission fields, `None` where the model has none.
fn fields(model: &str) -> (Option<&'static str>, Option<&'static str>) {
    match model {
        "standard_surface" => (Some("coat"), Some("transmission")),
        "open_pbr_surface" => (Some("coat_weight"), Some("transmission_weight")),
        "gltf_pbr" => (Some("clearcoat"), Some("transmission")),
        _ => (None, None),
    }
}

/// Whether the reflection's surface field `name` may be other than 0.
fn drives(reflection: &Reflection, name: &str) -> bool {
    let Some(param) = reflection.params.get(name) else {
        return false;
    };
    let value = match (param.kind, &param.uniform) {
        ("constant", _) => param.value.as_ref(),
        ("uniform", Some(uniform)) => reflection.uniforms.iter().find(|u| u.name == *uniform).map(|u| &u.default),
        _ => None,
    };
    !matches!(value, Some(Value::Float(x)) if *x == 0.0)
}

/// `base` with clearcoat and specular transmission turned on where the reflection's graph drives them.
pub fn enable(mut base: StandardMaterial, reflection: &Reflection) -> StandardMaterial {
    let (coat, transmission) = fields(&reflection.model);
    if base.clearcoat <= 0.0 && coat.is_some_and(|f| drives(reflection, f)) {
        base.clearcoat = ENABLED;
    }
    if base.specular_transmission <= 0.0 && transmission.is_some_and(|f| drives(reflection, f)) {
        base.specular_transmission = ENABLED;
    }
    base
}

#[cfg(test)]
mod tests {
    use super::*;
    use shadewire_mtlx::{MtlxOptions, to_ir};
    use shadewire_wgsl::compile;

    use crate::shader::compile_options;

    fn reflection(model: &str, inputs: &str) -> Reflection {
        let text = format!(
            r#"<materialx version="1.39">
  <texcoord name="uv" type="vector2" />
  <extract name="u" type="float">
    <input name="in" type="vector2" nodename="uv" />
    <input name="index" type="integer" value="0" />
  </extract>
  <{model} name="S" type="surfaceshader">
{inputs}
  </{model}>
  <surfacematerial name="M" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="S" />
  </surfacematerial>
</materialx>"#
        );
        let ir = to_ir(&text, None, None, &MtlxOptions::default()).unwrap();
        compile(&ir.dag, &ir.source, &compile_options()).unwrap().reflection
    }

    fn layers(model: &str, inputs: &str, base: StandardMaterial) -> (f32, f32) {
        let m = enable(base, &reflection(model, inputs));
        (m.clearcoat, m.specular_transmission)
    }

    #[test]
    fn layers_at_zero_stay_off() {
        assert_eq!(layers("standard_surface", "", StandardMaterial::default()), (0.0, 0.0));
        assert_eq!(layers("open_pbr_surface", "", StandardMaterial::default()), (0.0, 0.0));
        assert_eq!(layers("gltf_pbr", "", StandardMaterial::default()), (0.0, 0.0));
        assert_eq!(layers("surface_unlit", "", StandardMaterial::default()), (0.0, 0.0));
    }

    #[test]
    fn a_coat_or_transmission_value_turns_the_layer_on() {
        let coat = r#"    <input name="coat" type="float" value="0.8" />"#;
        assert_eq!(layers("standard_surface", coat, StandardMaterial::default()), (ENABLED, 0.0));
        let transmission = r#"    <input name="transmission_weight" type="float" value="1" />"#;
        assert_eq!(layers("open_pbr_surface", transmission, StandardMaterial::default()), (0.0, ENABLED));
        let both = r#"    <input name="clearcoat" type="float" value="1" />
    <input name="transmission" type="float" value="0.5" />"#;
        assert_eq!(layers("gltf_pbr", both, StandardMaterial::default()), (ENABLED, ENABLED));
    }

    #[test]
    fn a_varying_field_turns_the_layer_on() {
        let coat = r#"    <input name="coat_weight" type="float" nodename="u" />"#;
        assert_eq!(layers("open_pbr_surface", coat, StandardMaterial::default()), (ENABLED, 0.0));
    }

    #[test]
    fn a_base_that_turns_a_layer_on_keeps_its_factor() {
        let coat = r#"    <input name="coat" type="float" value="0.8" />"#;
        let base = StandardMaterial { clearcoat: 0.5, specular_transmission: 0.25, ..Default::default() };
        assert_eq!(layers("standard_surface", coat, base), (0.5, 0.25));
    }
}
