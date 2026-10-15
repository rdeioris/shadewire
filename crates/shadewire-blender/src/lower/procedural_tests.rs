use shadewire::Value;

use super::tests::{close, eval_field, lower, lower_field};

const XYZ: &str = r#"{"name": "X", "type": "ShaderNodeCombineXYZ", "mute": false, "properties": {},
  "inputs": [{"identifier": "X", "name": "X", "type": "VALUE", "enabled": true, "value": 0.3}, {"identifier": "Y", "name": "Y", "type": "VALUE", "enabled": true, "value": 0.6},
             {"identifier": "Z", "name": "Z", "type": "VALUE", "enabled": true, "value": 0.0}],
  "outputs": [{"identifier": "Vector", "name": "Vector", "type": "VECTOR", "enabled": true}], "internal_links": []}"#;

fn gradient(kind: &str) -> String {
    format!(
        r#"{{"name": "G", "type": "ShaderNodeTexGradient", "mute": false, "properties": {{"gradient_type": "{kind}"}},
      "inputs": [{{"identifier": "Vector", "name": "Vector", "type": "VECTOR", "enabled": true, "value": [0,0,0]}}],
      "outputs": [{{"identifier": "Color", "name": "Color", "type": "RGBA", "enabled": true}}, {{"identifier": "Fac", "name": "Fac", "type": "VALUE", "enabled": true}}], "internal_links": []}}"#
    )
}

#[test]
fn gradients_over_a_constant_vector_fold_to_blenders_values() {
    let link = r#"[[["X", "Vector"], ["G", "Vector"]], [["G", "Fac"], ["Principled BSDF", "Roughness"]]]"#;
    for (kind, expected) in [
        ("LINEAR", 0.3f32),
        ("QUADRATIC", 0.09),
        ("EASING", 3.0 * 0.09 - 2.0 * 0.027),
        ("DIAGONAL", 0.45),
        ("SPHERICAL", 1.0 - (0.09f32 + 0.36).sqrt()),
        ("RADIAL", (0.6f32).atan2(0.3) / (2.0 * std::f32::consts::PI) + 0.5),
    ] {
        let (value, warnings) = lower_field(&format!("{XYZ}, {}", gradient(kind)), link, "specular_roughness");
        assert!(warnings.is_empty(), "{kind}: {warnings:?}");
        assert!(close(&value, &Value::Float(expected)), "{kind}: {value:?}, Blender says {expected}");
    }
    let (value, _) = lower_field(&format!("{XYZ}, {}", gradient("QUADRATIC_SPHERE")), link, "specular_roughness");
    let s = 1.0 - (0.09f32 + 0.36).sqrt();
    assert!(close(&value, &Value::Float(s * s)));
    let (dag, warnings) = lower(&gradient("LINEAR"), r#"[[["G", "Fac"], ["Principled BSDF", "Roughness"]]]"#);
    assert_eq!(warnings, [r#"ShaderNodeTexGradient "G": unlinked Vector read as object position"#]);
    assert!(close(&eval_field(&dag, "specular_roughness", 0), &Value::Float(0.1)), "probe 0's object position x");
}

fn noise(dimensions: &str, normalize: bool, detail: f32) -> String {
    format!(
        r#"{{"name": "N", "type": "ShaderNodeTexNoise", "mute": false, "properties": {{"noise_dimensions": "{dimensions}", "noise_type": "FBM", "normalize": {normalize}}},
      "inputs": [{{"identifier": "Vector", "name": "Vector", "type": "VECTOR", "enabled": true, "value": [0,0,0]}}, {{"identifier": "W", "name": "W", "type": "VALUE", "enabled": true, "value": 0.0}},
                 {{"identifier": "Scale", "name": "Scale", "type": "VALUE", "enabled": true, "value": 5.0}}, {{"identifier": "Detail", "name": "Detail", "type": "VALUE", "enabled": true, "value": {detail}}},
                 {{"identifier": "Roughness", "name": "Roughness", "type": "VALUE", "enabled": true, "value": 0.5}}, {{"identifier": "Lacunarity", "name": "Lacunarity", "type": "VALUE", "enabled": true, "value": 2.0}},
                 {{"identifier": "Distortion", "name": "Distortion", "type": "VALUE", "enabled": true, "value": 0.0}}],
      "outputs": [{{"identifier": "Fac", "name": "Fac", "type": "VALUE", "enabled": true}}, {{"identifier": "Color", "name": "Color", "type": "RGBA", "enabled": true}}], "internal_links": []}}"#
    )
}

#[test]
fn noise_is_a_fractal_over_the_scaled_position() {
    let link = r#"[[["N", "Fac"], ["Principled BSDF", "Roughness"]]]"#;
    let (dag, warnings) = lower(&noise("3D", true, 2.0), link);
    assert_eq!(warnings, [r#"ShaderNodeTexNoise "N": unlinked Vector read as object position"#]);
    let values: Vec<f32> = (0..4)
        .map(|k| {
            let Value::Float(x) = eval_field(&dag, "specular_roughness", k) else { panic!() };
            x
        })
        .collect();
    assert!(values.iter().all(|x| (0.0..=1.0).contains(x)), "{values:?}");
    assert!(values.windows(2).any(|w| (w[0] - w[1]).abs() > 1e-4), "varies with the position: {values:?}");
    let (raw, _) = lower(&noise("3D", false, 2.0), link);
    let Value::Float(r) = eval_field(&raw, "specular_roughness", 1) else { panic!() };
    assert!(close(&Value::Float(values[1]), &Value::Float(r * 0.5 + 0.5)), "normalize is x/2 + 1/2");
    let (two_d, warnings) = lower(&noise("2D", true, 2.0), link);
    assert_eq!(warnings.len(), 1);
    assert_ne!(eval_field(&two_d, "specular_roughness", 1), eval_field(&dag, "specular_roughness", 1));
    let (_, warnings) = lower(&noise("4D", true, 2.0), link);
    assert!(warnings.iter().any(|w| w == r#"ShaderNodeTexNoise "N": 4D noise read as 3D (W dropped)"#), "{warnings:?}");
    let (dag, _) = lower(&noise("3D", true, 2.0), r#"[[["N", "Color"], ["Principled BSDF", "Base Color"]]]"#);
    let Value::Color3(c) = eval_field(&dag, "base_color", 0) else { panic!() };
    assert!(c.iter().all(|x| (0.0..=1.0).contains(x)) && (c[0] != c[1] || c[1] != c[2]), "{c:?}");
}

#[test]
fn voronoi_white_noise_and_checker() {
    let voronoi = |feature: &str| {
        format!(
            r#"{{"name": "V", "type": "ShaderNodeTexVoronoi", "mute": false, "properties": {{"voronoi_dimensions": "3D", "distance": "EUCLIDEAN", "feature": "{feature}", "normalize": false}},
      "inputs": [{{"identifier": "Vector", "name": "Vector", "type": "VECTOR", "enabled": true, "value": [0,0,0]}}, {{"identifier": "Scale", "name": "Scale", "type": "VALUE", "enabled": true, "value": 5.0}},
                 {{"identifier": "Randomness", "name": "Randomness", "type": "VALUE", "enabled": true, "value": 1.0}}],
      "outputs": [{{"identifier": "Distance", "name": "Distance", "type": "VALUE", "enabled": true}}, {{"identifier": "Color", "name": "Color", "type": "RGBA", "enabled": true}}], "internal_links": []}}"#
        )
    };
    let link = r#"[[["X", "Vector"], ["V", "Vector"]], [["V", "Distance"], ["Principled BSDF", "Roughness"]]]"#;
    let (dag, warnings) = lower(&format!("{XYZ}, {}", voronoi("F1")), link);
    assert!(warnings.is_empty(), "{warnings:?}");
    let Value::Float(f1) = eval_field(&dag, "specular_roughness", 0) else { panic!() };
    let (dag, _) = lower(&format!("{XYZ}, {}", voronoi("F2")), link);
    let Value::Float(f2) = eval_field(&dag, "specular_roughness", 0) else { panic!() };
    assert!(f1 >= 0.0 && f2 >= f1, "F2 is the second-nearest distance: {f1} {f2}");
    let (_, warnings) = lower(&format!("{XYZ}, {}", voronoi("SMOOTH_F1")), link);
    assert_eq!(warnings, [r#"ShaderNodeTexVoronoi "V": feature SMOOTH_F1 read as F1"#]);
    let (_, warnings) = lower(
        &format!("{XYZ}, {}", voronoi("F1")),
        r#"[[["X", "Vector"], ["V", "Vector"]], [["V", "Color"], ["Principled BSDF", "Base Color"]]]"#,
    );
    assert_eq!(warnings, [r#"ShaderNodeTexVoronoi "V": Color read as a gray cell noise"#]);
    let white = r#"{"name": "W", "type": "ShaderNodeTexWhiteNoise", "mute": false, "properties": {"noise_dimensions": "3D"},
      "inputs": [{"identifier": "Vector", "name": "Vector", "type": "VECTOR", "enabled": true, "value": [0,0,0]}],
      "outputs": [{"identifier": "Value", "name": "Value", "type": "VALUE", "enabled": true}, {"identifier": "Color", "name": "Color", "type": "RGBA", "enabled": true}], "internal_links": []}"#;
    let (dag, _) = lower(
        &format!("{XYZ}, {white}"),
        r#"[[["X", "Vector"], ["W", "Vector"]], [["W", "Value"], ["Principled BSDF", "Roughness"]]]"#,
    );
    let Value::Float(w) = eval_field(&dag, "specular_roughness", 0) else { panic!() };
    assert!((0.0..=1.0).contains(&w));
    let checker = r#"{"name": "C", "type": "ShaderNodeTexChecker", "mute": false, "properties": {},
      "inputs": [{"identifier": "Vector", "name": "Vector", "type": "VECTOR", "enabled": true, "value": [0,0,0]}, {"identifier": "Color1", "name": "Color1", "type": "RGBA", "enabled": true, "value": [1, 0, 0, 1]},
                 {"identifier": "Color2", "name": "Color2", "type": "RGBA", "enabled": true, "value": [0, 0, 1, 1]}, {"identifier": "Scale", "name": "Scale", "type": "VALUE", "enabled": true, "value": 4.0}],
      "outputs": [{"identifier": "Color", "name": "Color", "type": "RGBA", "enabled": true}, {"identifier": "Fac", "name": "Fac", "type": "VALUE", "enabled": true}], "internal_links": []}"#;
    let (dag, warnings) = lower(
        checker,
        r#"[[["C", "Color"], ["Principled BSDF", "Base Color"]], [["C", "Fac"], ["Principled BSDF", "Roughness"]]]"#,
    );
    assert_eq!(warnings, [r#"ShaderNodeTexChecker "C": unlinked Vector read as UV"#]);
    for k in 0..4 {
        let color = eval_field(&dag, "base_color", k);
        let fac = eval_field(&dag, "specular_roughness", k);
        let is_one = close(&color, &Value::Color3([1.0, 0.0, 0.0]));
        assert!(is_one || close(&color, &Value::Color3([0.0, 0.0, 1.0])), "{color:?}");
        assert_eq!(fac, Value::Float(if is_one { 1.0 } else { 0.0 }), "Fac follows Color1");
    }
}

#[test]
fn light_path_is_constants_with_one_warning() {
    let light = r#"{"name": "L", "type": "ShaderNodeLightPath", "mute": false, "properties": {}, "inputs": [],
      "outputs": [{"identifier": "Is Camera Ray", "name": "Is Camera Ray", "type": "VALUE", "enabled": true, "value": 0.0}, {"identifier": "Ray Length", "name": "Ray Length", "type": "VALUE", "enabled": true, "value": 0.0}], "internal_links": []}"#;
    let (value, warnings) = lower_field(
        light,
        r#"[[["L", "Is Camera Ray"], ["Principled BSDF", "Alpha"]], [["L", "Ray Length"], ["Principled BSDF", "Roughness"]]]"#,
        "geometry_opacity",
    );
    assert_eq!(value, Value::Float(1.0));
    assert_eq!(warnings, [r#"ShaderNodeLightPath "L": constants: Is Camera Ray 1, the rest 0"#]);
    let (value, _) =
        lower_field(light, r#"[[["L", "Ray Length"], ["Principled BSDF", "Roughness"]]]"#, "specular_roughness");
    assert_eq!(value, Value::Float(0.0));
}

#[test]
fn every_dimension_of_noise_and_voronoi_lowers_and_evaluates() {
    let voronoi = |dimensions: &str| {
        format!(
            r#"{{"name": "V", "type": "ShaderNodeTexVoronoi", "mute": false, "properties": {{"voronoi_dimensions": "{dimensions}", "distance": "EUCLIDEAN", "feature": "F2", "normalize": false}},
      "inputs": [{{"identifier": "Vector", "name": "Vector", "type": "VECTOR", "enabled": true, "value": [0,0,0]}}, {{"identifier": "W", "name": "W", "type": "VALUE", "enabled": true, "value": 0.5}},
                 {{"identifier": "Scale", "name": "Scale", "type": "VALUE", "enabled": true, "value": 5.0}}, {{"identifier": "Randomness", "name": "Randomness", "type": "VALUE", "enabled": true, "value": 1.0}}],
      "outputs": [{{"identifier": "Distance", "name": "Distance", "type": "VALUE", "enabled": true}}, {{"identifier": "Color", "name": "Color", "type": "RGBA", "enabled": true}}], "internal_links": []}}"#
        )
    };
    for dimensions in ["1D", "2D", "3D", "4D"] {
        let links = r#"[[["N", "Fac"], ["Principled BSDF", "Roughness"]], [["N", "Color"], ["Principled BSDF", "Base Color"]]]"#;
        let (dag, _) = lower(&noise(dimensions, true, 4.5), links);
        let Value::Float(fac) = eval_field(&dag, "specular_roughness", 1) else { panic!() };
        assert!(fac.is_finite(), "noise {dimensions}");
        eval_field(&dag, "base_color", 1);
        let links = r#"[[["V", "Distance"], ["Principled BSDF", "Roughness"]], [["V", "Color"], ["Principled BSDF", "Base Color"]]]"#;
        let (dag, _) = lower(&voronoi(dimensions), links);
        let Value::Float(distance) = eval_field(&dag, "specular_roughness", 1) else { panic!() };
        assert!(distance >= 0.0, "voronoi {dimensions}");
        eval_field(&dag, "base_color", 1);
    }
    let links = r#"[[["N", "Fac"], ["Principled BSDF", "Roughness"]]]"#;
    let (_, warnings) = lower(&noise("1D", true, 2.0), links);
    assert_eq!(warnings, [r#"ShaderNodeTexNoise "N": 1D noise read as 3D over (W, 0, 0)"#]);
    let (more, _) = lower(&noise("3D", false, 5.0).replace("\"value\": 2.0}", "\"value\": 2.3}"), links);
    let (fewer, _) = lower(&noise("3D", false, 0.0).replace("\"value\": 2.0}", "\"value\": 2.3}"), links);
    assert_ne!(
        eval_field(&more, "specular_roughness", 1),
        eval_field(&fewer, "specular_roughness", 1),
        "Detail adds octaves"
    );
}
