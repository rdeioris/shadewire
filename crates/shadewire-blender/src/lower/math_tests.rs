use shadewire::Value;

use super::tests::{close, lower_field};

fn math(op: &str, a: f32, b: f32, c: f32, clamp: bool) -> String {
    format!(
        r#"{{"name": "M", "type": "ShaderNodeMath", "mute": false, "properties": {{"operation": "{op}", "use_clamp": {clamp}}},
      "inputs": [{{"identifier": "Value", "name": "Value", "type": "VALUE", "enabled": true, "value": {a}}},
                 {{"identifier": "Value_001", "name": "Value", "type": "VALUE", "enabled": true, "value": {b}}},
                 {{"identifier": "Value_002", "name": "Value", "type": "VALUE", "enabled": true, "value": {c}}}],
      "outputs": [{{"identifier": "Value", "name": "Value", "type": "VALUE", "enabled": true}}], "internal_links": []}}"#
    )
}

fn math_value(op: &str, a: f32, b: f32, c: f32, clamp: bool) -> f32 {
    let (value, warnings) = lower_field(
        &math(op, a, b, c, clamp),
        r#"[[["M", "Value"], ["Principled BSDF", "Roughness"]]]"#,
        "specular_roughness",
    );
    assert!(warnings.is_empty(), "{op}: {warnings:?}");
    let Value::Float(x) = value else { panic!("{op}: {value:?}") };
    x
}

/// `(operation, a, b, c, expected)`: Blender's own results.
const CASES: &[(&str, f32, f32, f32, f32)] = &[
    ("ADD", 1.5, 2.0, 0.0, 3.5),
    ("SUBTRACT", 1.5, 2.0, 0.0, -0.5),
    ("MULTIPLY", 1.5, 2.0, 0.0, 3.0),
    ("DIVIDE", 3.0, 2.0, 0.0, 1.5),
    ("DIVIDE", 3.0, 0.0, 0.0, 0.0),
    ("MULTIPLY_ADD", 2.0, 3.0, 1.0, 7.0),
    ("POWER", 2.0, 3.0, 0.0, 8.0),
    ("LOGARITHM", 8.0, 2.0, 0.0, 3.0),
    ("SQRT", 9.0, 0.0, 0.0, 3.0),
    ("SQRT", -4.0, 0.0, 0.0, 0.0),
    ("INVERSE_SQRT", 4.0, 0.0, 0.0, 0.5),
    ("INVERSE_SQRT", 0.0, 0.0, 0.0, 0.0),
    ("ABSOLUTE", -2.5, 0.0, 0.0, 2.5),
    ("EXPONENT", 0.0, 0.0, 0.0, 1.0),
    ("MINIMUM", 1.0, 2.0, 0.0, 1.0),
    ("MAXIMUM", 1.0, 2.0, 0.0, 2.0),
    ("LESS_THAN", 1.0, 2.0, 0.0, 1.0),
    ("GREATER_THAN", 1.0, 2.0, 0.0, 0.0),
    ("SIGN", -3.0, 0.0, 0.0, -1.0),
    ("COMPARE", 1.0, 1.05, 0.1, 1.0),
    ("COMPARE", 1.0, 1.5, 0.1, 0.0),
    ("SMOOTH_MIN", 1.0, 1.2, 0.5, 1.0 - 0.6 * 0.6 * 0.6 * 0.5 / 6.0),
    ("SMOOTH_MIN", 1.0, 2.0, 0.0, 1.0),
    ("SMOOTH_MAX", 1.0, 1.2, 0.5, 1.2 + 0.6 * 0.6 * 0.6 * 0.5 / 6.0),
    ("ROUND", 2.5, 0.0, 0.0, 3.0),
    ("ROUND", -2.5, 0.0, 0.0, -2.0),
    ("FLOOR", -1.5, 0.0, 0.0, -2.0),
    ("CEIL", -1.5, 0.0, 0.0, -1.0),
    ("TRUNC", -1.5, 0.0, 0.0, -1.0),
    ("FRACT", -1.25, 0.0, 0.0, 0.75),
    ("MODULO", -7.0, 3.0, 0.0, -1.0),
    ("FLOORED_MODULO", -7.0, 3.0, 0.0, 2.0),
    ("MODULO", 7.0, 0.0, 0.0, 0.0),
    ("WRAP", 7.0, 5.0, 2.0, 4.0),
    ("WRAP", 1.0, 2.0, 2.0, 2.0),
    ("SNAP", 7.3, 0.5, 0.0, 7.0),
    ("SNAP", 7.3, 0.0, 0.0, 0.0),
    ("PINGPONG", 3.5, 2.0, 0.0, 0.5),
    ("PINGPONG", 1.5, 2.0, 0.0, 1.5),
    ("PINGPONG", 1.0, 0.0, 0.0, 0.0),
    ("SINE", 0.0, 0.0, 0.0, 0.0),
    ("COSINE", 0.0, 0.0, 0.0, 1.0),
    ("TANGENT", 0.0, 0.0, 0.0, 0.0),
    ("ARCSINE", 1.0, 0.0, 0.0, std::f32::consts::FRAC_PI_2),
    ("ARCCOSINE", 1.0, 0.0, 0.0, 0.0),
    ("ARCTANGENT", 1.0, 0.0, 0.0, std::f32::consts::FRAC_PI_4),
    ("ARCTAN2", 1.0, 1.0, 0.0, std::f32::consts::FRAC_PI_4),
    ("SINH", 1.0, 0.0, 0.0, 1.1752012),
    ("COSH", 1.0, 0.0, 0.0, 1.5430806),
    ("TANH", 1.0, 0.0, 0.0, 0.7615942),
    ("RADIANS", 180.0, 0.0, 0.0, std::f32::consts::PI),
    ("DEGREES", std::f32::consts::PI, 0.0, 0.0, 180.0),
];

#[test]
fn every_math_operation_gives_blenders_result() {
    for (op, a, b, c, expected) in CASES {
        let got = math_value(op, *a, *b, *c, false);
        assert!((got - expected).abs() <= 1e-4, "{op}({a}, {b}, {c}) = {got}, Blender says {expected}");
    }
}

#[test]
fn use_clamp_clamps_the_result() {
    assert_eq!(math_value("ADD", 1.5, 2.0, 0.0, true), 1.0);
    assert_eq!(math_value("SUBTRACT", 1.0, 2.0, 0.0, true), 0.0);
}

fn vector_math(op: &str, a: [f32; 3], b: [f32; 3], c: [f32; 3], s: f32) -> String {
    let v = |x: [f32; 3]| format!("[{}, {}, {}]", x[0], x[1], x[2]);
    format!(
        r#"{{"name": "VM", "type": "ShaderNodeVectorMath", "mute": false, "properties": {{"operation": "{op}"}},
      "inputs": [{{"identifier": "Vector", "name": "Vector", "type": "VECTOR", "enabled": true, "value": {}}},
                 {{"identifier": "Vector_001", "name": "Vector", "type": "VECTOR", "enabled": true, "value": {}}},
                 {{"identifier": "Vector_002", "name": "Vector", "type": "VECTOR", "enabled": true, "value": {}}},
                 {{"identifier": "Scale", "name": "Scale", "type": "VALUE", "enabled": true, "value": {s}}}],
      "outputs": [{{"identifier": "Vector", "name": "Vector", "type": "VECTOR", "enabled": true}},
                  {{"identifier": "Value", "name": "Value", "type": "VALUE", "enabled": true}}], "internal_links": []}}"#,
        v(a),
        v(b),
        v(c)
    )
}

#[test]
fn vector_math_operations_give_blenders_results() {
    // (operation, a, b, c, scale, expected) through the Subsurface Radius field (a vector3 read as color3).
    type Case = (&'static str, [f32; 3], [f32; 3], [f32; 3], f32, [f32; 3]);
    let vector_cases: &[Case] = &[
        ("ADD", [1.0, 2.0, 3.0], [1.0, 1.0, 1.0], [0.0; 3], 1.0, [2.0, 3.0, 4.0]),
        ("CROSS_PRODUCT", [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0; 3], 1.0, [0.0, 0.0, 1.0]),
        ("PROJECT", [1.0, 1.0, 0.0], [2.0, 0.0, 0.0], [0.0; 3], 1.0, [1.0, 0.0, 0.0]),
        ("REFLECT", [1.0, -1.0, 0.0], [0.0, 2.0, 0.0], [0.0; 3], 1.0, [1.0, 1.0, 0.0]),
        ("FACEFORWARD", [1.0, 2.0, 3.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], 1.0, [-1.0, -2.0, -3.0]),
        ("SCALE", [1.0, 2.0, 3.0], [0.0; 3], [0.0; 3], 2.0, [2.0, 4.0, 6.0]),
        ("NORMALIZE", [0.0, 3.0, 4.0], [0.0; 3], [0.0; 3], 1.0, [0.0, 0.6, 0.8]),
        ("MODULO", [-7.0, 7.0, 5.0], [3.0, 3.0, 2.0], [0.0; 3], 1.0, [-1.0, 1.0, 1.0]),
        ("WRAP", [7.0, 1.0, 6.0], [5.0, 5.0, 5.0], [2.0, 2.0, 2.0], 1.0, [4.0, 4.0, 3.0]),
        ("SNAP", [7.3, 1.2, 0.0], [0.5, 1.0, 1.0], [0.0; 3], 1.0, [7.0, 1.0, 0.0]),
        ("MULTIPLY_ADD", [1.0, 2.0, 3.0], [2.0, 2.0, 2.0], [1.0, 1.0, 1.0], 1.0, [3.0, 5.0, 7.0]),
    ];
    for (op, a, b, c, s, expected) in vector_cases {
        let (value, warnings) = lower_field(
            &vector_math(op, *a, *b, *c, *s),
            r#"[[["VM", "Vector"], ["Principled BSDF", "Subsurface Radius"]]]"#,
            "subsurface_radius_scale",
        );
        assert!(warnings.is_empty(), "{op}: {warnings:?}");
        assert!(close(&value, &Value::Color3(*expected)), "{op}: {value:?}, Blender says {expected:?}");
    }
    for (op, a, b, expected) in [
        ("DOT_PRODUCT", [1.0, 2.0, 3.0], [4.0, 5.0, 6.0], 32.0f32),
        ("DISTANCE", [0.0, 0.0, 0.0], [3.0, 4.0, 0.0], 5.0),
        ("LENGTH", [0.0, 3.0, 4.0], [0.0; 3], 5.0),
    ] {
        let (value, _) = lower_field(
            &vector_math(op, a, b, [0.0; 3], 1.0),
            r#"[[["VM", "Value"], ["Principled BSDF", "Roughness"]]]"#,
            "specular_roughness",
        );
        assert!(close(&value, &Value::Float(expected)), "{op}: {value:?}");
    }
    let (value, warnings) = lower_field(
        &vector_math("ADD", [1.0; 3], [1.0; 3], [0.0; 3], 1.0),
        r#"[[["VM", "Value"], ["Principled BSDF", "Roughness"]]]"#,
        "specular_roughness",
    );
    assert_eq!(value, Value::Float(0.0));
    assert_eq!(warnings, [r#"ShaderNodeVectorMath "VM": output Value is not produced by ADD, zero"#]);
}

#[test]
fn clamp_and_map_range_follow_blender() {
    let clamp = |kind: &str, v: f32, lo: f32, hi: f32| {
        format!(
            r#"{{"name": "C", "type": "ShaderNodeClamp", "mute": false, "properties": {{"clamp_type": "{kind}"}},
      "inputs": [{{"identifier": "Value", "name": "Value", "type": "VALUE", "enabled": true, "value": {v}}},
                 {{"identifier": "Min", "name": "Min", "type": "VALUE", "enabled": true, "value": {lo}}},
                 {{"identifier": "Max", "name": "Max", "type": "VALUE", "enabled": true, "value": {hi}}}],
      "outputs": [{{"identifier": "Result", "name": "Result", "type": "VALUE", "enabled": true}}], "internal_links": []}}"#
        )
    };
    let link = r#"[[["C", "Result"], ["Principled BSDF", "Roughness"]]]"#;
    assert_eq!(lower_field(&clamp("MINMAX", 1.5, 0.0, 1.0), link, "specular_roughness").0, Value::Float(1.0));
    // MINMAX does not reorder an inverted range: Blender 5.2 gives min(max(0.5, 1), 0) = 0.
    assert_eq!(lower_field(&clamp("MINMAX", 0.5, 1.0, 0.0), link, "specular_roughness").0, Value::Float(0.0));
    assert_eq!(lower_field(&clamp("RANGE", 0.5, 1.0, 0.0), link, "specular_roughness").0, Value::Float(0.5));
    let map = |interp: &str, clamp: bool, v: f32, steps: f32| {
        format!(
            r#"{{"name": "R", "type": "ShaderNodeMapRange", "mute": false,
      "properties": {{"clamp": {clamp}, "interpolation_type": "{interp}", "data_type": "FLOAT"}},
      "inputs": [{{"identifier": "Value", "name": "Value", "type": "VALUE", "enabled": true, "value": {v}}},
                 {{"identifier": "From Min", "name": "From Min", "type": "VALUE", "enabled": true, "value": 0.0}},
                 {{"identifier": "From Max", "name": "From Max", "type": "VALUE", "enabled": true, "value": 2.0}},
                 {{"identifier": "To Min", "name": "To Min", "type": "VALUE", "enabled": true, "value": 10.0}},
                 {{"identifier": "To Max", "name": "To Max", "type": "VALUE", "enabled": true, "value": 20.0}},
                 {{"identifier": "Steps", "name": "Steps", "type": "VALUE", "enabled": true, "value": {steps}}}],
      "outputs": [{{"identifier": "Result", "name": "Result", "type": "VALUE", "enabled": true}}], "internal_links": []}}"#
        )
    };
    let link = r#"[[["R", "Result"], ["Principled BSDF", "Emission Strength"]]]"#;
    let get = |nodes: String| {
        let Value::Float(x) = lower_field(&nodes, link, "emission_luminance").0 else { panic!() };
        x
    };
    assert!((get(map("LINEAR", false, 1.0, 4.0)) - 15.0).abs() < 1e-4);
    assert!((get(map("LINEAR", false, 3.0, 4.0)) - 25.0).abs() < 1e-4);
    assert!((get(map("LINEAR", true, 3.0, 4.0)) - 20.0).abs() < 1e-4);
    assert!((get(map("STEPPED", false, 1.0, 4.0)) - 15.0).abs() < 1e-4); // t = 0.5, floor(0.5 * 5) / 4 = 0.5
    assert!((get(map("STEPPED", false, 0.7, 4.0)) - 12.5).abs() < 1e-4); // t = 0.35, floor(0.35 * 5) / 4 = 0.25
    assert!((get(map("SMOOTHSTEP", false, 0.5, 4.0)) - (10.0 + 10.0 * (3.0 * 0.0625 - 2.0 * 0.015625))).abs() < 1e-4);
    assert!((get(map("SMOOTHERSTEP", false, 1.0, 4.0)) - 15.0).abs() < 1e-4);
}

#[test]
fn an_unknown_math_operation_passes_the_first_value_through_with_a_warning() {
    let link = r#"[[["M", "Value"], ["Principled BSDF", "Roughness"]]]"#;
    let (value, warnings) = lower_field(&math("FUTURE_OP", 0.25, 2.0, 0.0, false), link, "specular_roughness");
    assert_eq!(value, Value::Float(0.25));
    assert_eq!(warnings, [r#"ShaderNodeMath "M": operation FUTURE_OP not lowered, the first value passes through"#]);
}

#[test]
fn map_range_over_vectors_remaps_and_writes_other_interpolations_as_linear() {
    let socket = |id: &str, v: &str| {
        format!(
            r#"{{"identifier": "{id}", "name": "{id}", "type": "VECTOR", "enabled": true, "value": [{v}, {v}, {v}]}}"#
        )
    };
    let map = |interp: &str| {
        format!(
            r#"{{"name": "R", "type": "ShaderNodeMapRange", "mute": false,
          "properties": {{"clamp": false, "interpolation_type": "{interp}", "data_type": "FLOAT_VECTOR"}},
          "inputs": [{}, {}, {}, {}, {}],
          "outputs": [{{"identifier": "Vector", "name": "Vector", "type": "VECTOR", "enabled": true}}], "internal_links": []}}"#,
            socket("Vector", "1.0"),
            socket("From_Min_FLOAT3", "0.0"),
            socket("From_Max_FLOAT3", "2.0"),
            socket("To_Min_FLOAT3", "10.0"),
            socket("To_Max_FLOAT3", "20.0"),
        )
    };
    let link = r#"[[["R", "Vector"], ["Principled BSDF", "Subsurface Radius"]]]"#;
    let (value, warnings) = lower_field(&map("LINEAR"), link, "subsurface_radius_scale");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(close(&value, &Value::Color3([15.0; 3])), "{value:?}");
    let (value, warnings) = lower_field(&map("STEPPED"), link, "subsurface_radius_scale");
    assert!(close(&value, &Value::Color3([15.0; 3])), "{value:?}");
    assert_eq!(warnings, [r#"ShaderNodeMapRange "R": STEPPED over vectors written as Linear"#]);
}
