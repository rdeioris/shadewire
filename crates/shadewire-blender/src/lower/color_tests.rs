use shadewire::Value;

use super::tests::{close, lower_field};

fn mix(data_type: &str, blend: &str, fac: f32, a: &str, b: &str, clamp_result: bool) -> String {
    let (fa, fb) = match data_type {
        "RGBA" => ("A_Color", "B_Color"),
        "VECTOR" => ("A_Vector", "B_Vector"),
        _ => ("A_Float", "B_Float"),
    };
    let ty = match data_type {
        "RGBA" => "RGBA",
        "VECTOR" => "VECTOR",
        _ => "VALUE",
    };
    format!(
        r#"{{"name": "Mix", "type": "ShaderNodeMix", "mute": false,
      "properties": {{"data_type": "{data_type}", "factor_mode": "UNIFORM", "blend_type": "{blend}", "clamp_factor": true, "clamp_result": {clamp_result}}},
      "inputs": [{{"identifier": "Factor_Float", "name": "Factor", "type": "VALUE", "enabled": true, "value": {fac}}},
                 {{"identifier": "{fa}", "name": "A", "type": "{ty}", "enabled": true, "value": {a}}},
                 {{"identifier": "{fb}", "name": "B", "type": "{ty}", "enabled": true, "value": {b}}}],
      "outputs": [{{"identifier": "Result_Float", "name": "Result", "type": "VALUE", "enabled": true}},
                  {{"identifier": "Result_Vector", "name": "Result", "type": "VECTOR", "enabled": true}},
                  {{"identifier": "Result_Color", "name": "Result", "type": "RGBA", "enabled": true}}], "internal_links": []}}"#
    )
}

fn mixed(blend: &str, fac: f32, a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    let v = |x: [f32; 3]| format!("[{}, {}, {}, 1.0]", x[0], x[1], x[2]);
    let (value, warnings) = lower_field(
        &mix("RGBA", blend, fac, &v(a), &v(b), false),
        r#"[[["Mix", "Result_Color"], ["Principled BSDF", "Base Color"]]]"#,
        "base_color",
    );
    assert!(warnings.is_empty(), "{blend}: {warnings:?}");
    let Value::Color3(c) = value else { panic!("{blend}: {value:?}") };
    c
}

/// Blender's results (`MA_RAMP_*` in blender/source/blender/blenkernel/intern/material.cc) at fac 0.5.
#[test]
fn every_color_blend_gives_blenders_result() {
    let (a, b, f) = ([0.2f32, 0.5, 0.8], [0.6f32, 0.4, 0.1], 0.5f32);
    let lerp =
        |x: [f32; 3], y: [f32; 3]| [x[0] + f * (y[0] - x[0]), x[1] + f * (y[1] - x[1]), x[2] + f * (y[2] - x[2])];
    let each = |g: &dyn Fn(f32, f32) -> f32| [g(a[0], b[0]), g(a[1], b[1]), g(a[2], b[2])];
    let cases: Vec<(&str, [f32; 3])> = vec![
        ("MIX", lerp(a, b)),
        ("MULTIPLY", lerp(a, each(&|x, y| x * y))),
        ("SCREEN", lerp(a, each(&|x, y| 1.0 - (1.0 - x) * (1.0 - y)))),
        ("OVERLAY", lerp(a, each(&|x, y| if x < 0.5 { 2.0 * x * y } else { 1.0 - 2.0 * (1.0 - x) * (1.0 - y) }))),
        ("ADD", lerp(a, each(&|x, y| x + y))),
        ("SUBTRACT", lerp(a, each(&|x, y| x - y))),
        ("DIFFERENCE", lerp(a, each(&|x, y| (x - y).abs()))),
        ("DARKEN", lerp(a, each(&f32::min))),
        ("LIGHTEN", lerp(a, each(&f32::max))),
        ("DIVIDE", lerp(a, each(&|x, y| if y != 0.0 { x / y } else { x }))),
        (
            "SOFT_LIGHT",
            lerp(
                a,
                each(&|x, y| {
                    let scr = 1.0 - (1.0 - y) * (1.0 - x);
                    (1.0 - x) * y * x + x * scr
                }),
            ),
        ),
        ("LINEAR_LIGHT", each(&|x, y| x + f * (2.0 * y - 1.0))),
        ("EXCLUSION", lerp(a, each(&|x, y| x + y - 2.0 * x * y))),
        (
            "BURN",
            each(&|x, y| {
                let t = 1.0 - f * (1.0 - y);
                if t <= 0.0 { 0.0 } else { (1.0 - (1.0 - x) / t).clamp(0.0, 1.0) }
            }),
        ),
        (
            "DODGE",
            each(&|x, y| {
                if x == 0.0 {
                    0.0
                } else {
                    let t = 1.0 - f * y;
                    if t <= 0.0 { 1.0 } else { (x / t).min(1.0) }
                }
            }),
        ),
    ];
    for (blend, expected) in cases {
        let got = mixed(blend, f, a, b);
        assert!(close(&Value::Color3(got), &Value::Color3(expected)), "{blend}: {got:?}, Blender says {expected:?}");
    }
    // HUE takes B's hue (red) with A's saturation and value (a blue of saturation 0.75, value 0.8).
    let hue = mixed("HUE", 1.0, [0.2, 0.2, 0.8], [0.9, 0.1, 0.1]);
    assert!(close(&Value::Color3(hue), &Value::Color3([0.8, 0.2, 0.2])), "{hue:?}");
    let value = mixed("VALUE", 1.0, [0.2, 0.2, 0.8], [0.4, 0.4, 0.4]);
    assert!(close(&Value::Color3(value), &Value::Color3([0.1, 0.1, 0.4])), "{value:?}");
}

#[test]
fn float_and_vector_mixes_and_the_clamps() {
    let (value, _) = lower_field(
        &mix("VALUE", "MIX", 0.25, "1.0", "3.0", false),
        r#"[[["Mix", "Result_Float"], ["Principled BSDF", "Roughness"]]]"#,
        "specular_roughness",
    );
    assert!(close(&value, &Value::Float(1.5)));
    let (value, _) = lower_field(
        &mix("VALUE", "MIX", 2.0, "1.0", "3.0", false),
        r#"[[["Mix", "Result_Float"], ["Principled BSDF", "Roughness"]]]"#,
        "specular_roughness",
    );
    assert!(close(&value, &Value::Float(3.0)), "clamp_factor: {value:?}");
    let (value, _) = lower_field(
        &mix("VECTOR", "MIX", 0.5, "[0, 0, 0]", "[2, 4, 6]", false),
        r#"[[["Mix", "Result_Vector"], ["Principled BSDF", "Subsurface Radius"]]]"#,
        "subsurface_radius_scale",
    );
    assert!(close(&value, &Value::Color3([1.0, 2.0, 3.0])));
    let (value, _) = lower_field(
        &mix("RGBA", "ADD", 1.0, "[0.5, 0.5, 0.5, 1]", "[0.8, 0.8, 0.8, 1]", true),
        r#"[[["Mix", "Result_Color"], ["Principled BSDF", "Base Color"]]]"#,
        "base_color",
    );
    assert!(close(&value, &Value::Color3([1.0; 3])), "clamp_result: {value:?}");
    let non_uniform = mix("VECTOR", "MIX", 0.5, "[0, 0, 0]", "[2, 4, 6]", false)
        .replace(r#""factor_mode": "UNIFORM""#, r#""factor_mode": "NON_UNIFORM""#)
        .replace(r#"{"identifier": "Factor_Float", "name": "Factor", "type": "VALUE", "enabled": true, "value": 0.5}"#,
                 r#"{"identifier": "Factor_Vector", "name": "Factor", "type": "VECTOR", "enabled": true, "value": [0.5, 0.25, 1.0]}"#);
    let (value, warnings) = lower_field(
        &non_uniform,
        r#"[[["Mix", "Result_Vector"], ["Principled BSDF", "Subsurface Radius"]]]"#,
        "subsurface_radius_scale",
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(close(&value, &Value::Color3([1.0, 1.0, 6.0])), "{value:?}");
}

#[test]
fn separate_and_combine_color_and_xyz() {
    let separate = |mode: &str| {
        format!(
            r#"{{"name": "S", "type": "ShaderNodeSeparateColor", "mute": false, "properties": {{"mode": "{mode}"}},
      "inputs": [{{"identifier": "Color", "name": "Color", "type": "RGBA", "enabled": true, "value": [0.2, 0.4, 0.8, 1.0]}}],
      "outputs": [{{"identifier": "Red", "name": "Red", "type": "VALUE", "enabled": true}}, {{"identifier": "Green", "name": "Green", "type": "VALUE", "enabled": true}},
                  {{"identifier": "Blue", "name": "Blue", "type": "VALUE", "enabled": true}}], "internal_links": []}}"#
        )
    };
    let link = |out: &str| format!(r#"[[["S", "{out}"], ["Principled BSDF", "Roughness"]]]"#);
    assert!(close(&lower_field(&separate("RGB"), &link("Green"), "specular_roughness").0, &Value::Float(0.4)));
    // HSV of (0.2, 0.4, 0.8): value 0.8, saturation 0.75.
    assert!(close(&lower_field(&separate("HSV"), &link("Blue"), "specular_roughness").0, &Value::Float(0.8)));
    assert!(close(&lower_field(&separate("HSV"), &link("Green"), "specular_roughness").0, &Value::Float(0.75)));
    let (_, warnings) = lower_field(&separate("HSL"), &link("Red"), "specular_roughness");
    assert_eq!(warnings, [r#"ShaderNodeSeparateColor "S": HSL read as HSV"#]);
    let combine = r#"{"name": "C", "type": "ShaderNodeCombineColor", "mute": false, "properties": {"mode": "RGB"},
      "inputs": [{"identifier": "Red", "name": "Red", "type": "VALUE", "enabled": true, "value": 0.1}, {"identifier": "Green", "name": "Green", "type": "VALUE", "enabled": true, "value": 0.2},
                 {"identifier": "Blue", "name": "Blue", "type": "VALUE", "enabled": true, "value": 0.3}],
      "outputs": [{"identifier": "Color", "name": "Color", "type": "RGBA", "enabled": true}], "internal_links": []}"#;
    assert!(close(
        &lower_field(combine, r#"[[["C", "Color"], ["Principled BSDF", "Base Color"]]]"#, "base_color").0,
        &Value::Color3([0.1, 0.2, 0.3])
    ));
    let xyz = r#"{"name": "X", "type": "ShaderNodeCombineXYZ", "mute": false, "properties": {},
      "inputs": [{"identifier": "X", "name": "X", "type": "VALUE", "enabled": true, "value": 1.0}, {"identifier": "Y", "name": "Y", "type": "VALUE", "enabled": true, "value": 2.0},
                 {"identifier": "Z", "name": "Z", "type": "VALUE", "enabled": true, "value": 3.0}],
      "outputs": [{"identifier": "Vector", "name": "Vector", "type": "VECTOR", "enabled": true}], "internal_links": []}"#;
    assert!(close(
        &lower_field(
            xyz,
            r#"[[["X", "Vector"], ["Principled BSDF", "Subsurface Radius"]]]"#,
            "subsurface_radius_scale"
        )
        .0,
        &Value::Color3([1.0, 2.0, 3.0])
    ));
}

#[test]
fn invert_gamma_brightness_contrast_and_hsv_follow_blender() {
    let node = |kind: &str, inputs: &str| {
        format!(
            r#"{{"name": "N", "type": "{kind}", "mute": false, "properties": {{}}, "inputs": [{inputs}],
      "outputs": [{{"identifier": "Color", "name": "Color", "type": "RGBA", "enabled": true}}], "internal_links": []}}"#
        )
    };
    let link = r#"[[["N", "Color"], ["Principled BSDF", "Base Color"]]]"#;
    let color =
        r#"{"identifier": "Color", "name": "Color", "type": "RGBA", "enabled": true, "value": [0.2, 0.5, 1.0, 1.0]}"#;
    let f = |id: &str, v: f32| {
        format!(r#"{{"identifier": "{id}", "name": "{id}", "type": "VALUE", "enabled": true, "value": {v}}}"#)
    };
    let invert = node("ShaderNodeInvert", &format!("{}, {color}", f("Fac", 0.5)));
    assert!(close(&lower_field(&invert, link, "base_color").0, &Value::Color3([0.5, 0.5, 0.5])));
    let gamma = node("ShaderNodeGamma", &format!("{color}, {}", f("Gamma", 2.0)));
    assert!(close(&lower_field(&gamma, link, "base_color").0, &Value::Color3([0.04, 0.25, 1.0])));
    let bc = node("ShaderNodeBrightContrast", &format!("{color}, {}, {}", f("Bright", 0.1), f("Contrast", 0.5)));
    // a = 1.5, b = 0.1 − 0.25 = −0.15: (0.2·1.5 − 0.15, 0.5·1.5 − 0.15, 1.0·1.5 − 0.15), floored at 0.
    assert!(close(&lower_field(&bc, link, "base_color").0, &Value::Color3([0.15, 0.6, 1.35])));
    let hsv = node(
        "ShaderNodeHueSaturation",
        &format!("{}, {}, {}, {}, {color}", f("Hue", 0.5), f("Saturation", 1.0), f("Value", 2.0), f("Fac", 1.0)),
    );
    assert!(close(&lower_field(&hsv, link, "base_color").0, &Value::Color3([0.4, 1.0, 2.0])));
}

#[test]
fn an_unknown_blend_is_a_mix_and_a_disabled_result_is_zero() {
    let (value, warnings) = lower_field(
        &mix("RGBA", "WARP", 0.5, "[0.2, 0.2, 0.2, 1]", "[0.6, 0.6, 0.6, 1]", false),
        r#"[[["Mix", "Result_Color"], ["Principled BSDF", "Base Color"]]]"#,
        "base_color",
    );
    assert_eq!(warnings, [r#"ShaderNodeMix "Mix": blend WARP not lowered, written as Mix"#]);
    assert!(close(&value, &Value::Color3([0.4; 3])), "{value:?}");
    let (value, warnings) = lower_field(
        &mix("VALUE", "MIX", 0.5, "1.0", "3.0", false),
        r#"[[["Mix", "Result_Color"], ["Principled BSDF", "Base Color"]]]"#,
        "base_color",
    );
    assert_eq!(warnings, [r#"ShaderNodeMix "Mix": output Result_Color is not produced over VALUE, zero"#]);
    assert!(close(&value, &Value::Color3([0.0; 3])), "{value:?}");
}
