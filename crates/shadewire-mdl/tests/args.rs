//! Typed arguments (`to_ir_with_args`): each kind a host passes, converted to its parameter's declared type and in
//! place of the default before evaluation, so folding sees it. The fixture is `tests/fixtures/args.mdl`.

use std::path::Path;

use shadewire::optimize::optimize;
use shadewire::{Dag, Error, LOBE_MODELS, NodeKind, Value};
use shadewire_mdl::{IrOptions, MaterialIr, MdlArgValue, MdlArgument, Modules, Resolver, to_ir_with_args};

fn text() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/args.mdl")).unwrap()
}

fn arg(name: &str, value: MdlArgValue) -> MdlArgument {
    MdlArgument { name: name.to_string(), value }
}

fn floats(v: &[f32]) -> MdlArgValue {
    MdlArgValue::Float(v.to_vec())
}

fn texture(path: &str, colorspace: Option<&str>) -> MdlArgValue {
    MdlArgValue::Texture { path: path.to_string(), colorspace: colorspace.map(str::to_string) }
}

fn lower(args: &[MdlArgument], uniforms: bool) -> Result<MaterialIr, Error> {
    let mut options = IrOptions::with_models(&LOBE_MODELS);
    options.values_as_uniforms = uniforms;
    to_ir_with_args(&text(), None, Some("args"), args, &options, &Resolver::default())
}

fn dag(args: &[MdlArgument], uniforms: bool) -> Dag {
    let ir = lower(args, uniforms).unwrap_or_else(|e| panic!("{e}"));
    optimize(&ir.dag)
}

/// What holds surface field `name`.
#[derive(Debug, PartialEq)]
enum Held {
    Const(Value),
    Uniform(String),
    Computed,
}

fn field(dag: &Dag, name: &str) -> Held {
    let f = dag.surface.fields.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("no field {name}"));
    match &dag.node(f.node).kind {
        NodeKind::Const(v) => Held::Const(v.clone()),
        NodeKind::Uniform { name, .. } => Held::Uniform(name.clone()),
        _ => Held::Computed,
    }
}

fn uniform(dag: &Dag, name: &str) -> Option<Value> {
    dag.nodes.iter().find_map(|n| match &n.kind {
        NodeKind::Uniform { name: n, value } if n == name => Some(value.clone()),
        _ => None,
    })
}

fn samples(dag: &Dag) -> usize {
    dag.nodes.iter().filter(|n| matches!(n.kind, NodeKind::Sample { .. })).count()
}

fn error(args: &[MdlArgument]) -> String {
    match lower(args, true) {
        Ok(_) => panic!("{args:?} lowered"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn the_defaults_without_arguments() {
    let d = dag(&[], false);
    assert_eq!(field(&d, "c1_color"), Held::Const(Value::Color3([0.5; 3])));
    assert_eq!(field(&d, "c0_ior"), Held::Const(Value::Float(1.5)));
    assert_eq!(field(&d, "c0_roughness"), Held::Const(Value::Vector2([0.4, 0.4])));
    assert_eq!(field(&d, "opacity"), Held::Const(Value::Float(1.0)));
    assert_eq!(field(&d, "thin_walled"), Held::Const(Value::Boolean(false)));
    assert!(d.textures.is_empty());
}

#[test]
fn each_argument_folds_into_a_constant() {
    let args = [
        arg("base_color", floats(&[0.1, 0.2, 0.3])),
        arg("roughness", floats(&[0.2, 0.3])),
        arg("ior", floats(&[1.3])),
        arg("thin", MdlArgValue::Bool(true)),
        arg("finish", MdlArgValue::Int(7)),
    ];
    let d = dag(&args, false);
    assert_eq!(field(&d, "c1_color"), Held::Const(Value::Color3([0.1, 0.2, 0.3])));
    assert_eq!(field(&d, "c0_roughness"), Held::Const(Value::Vector2([0.2, 0.3])));
    assert_eq!(field(&d, "c0_ior"), Held::Const(Value::Float(1.3)));
    assert_eq!(field(&d, "thin_walled"), Held::Const(Value::Boolean(true)));
    assert_eq!(field(&d, "opacity"), Held::Const(Value::Float(0.25)));
    // A string argument folds the comparison that reads it.
    let lacquer = dag(&[arg("preset", MdlArgValue::String("lacquer".into())), arg("ior", floats(&[1.3]))], false);
    assert_eq!(field(&lacquer, "c0_ior"), Held::Const(Value::Float(1.8)));
}

#[test]
fn with_values_as_uniforms_an_argument_is_the_uniform_default() {
    let args = [
        arg("base_color", floats(&[0.1, 0.2, 0.3])),
        arg("roughness", floats(&[0.2, 0.3])),
        arg("ior", floats(&[1.3])),
        arg("thin", MdlArgValue::Bool(true)),
        arg("finish", MdlArgValue::Int(3)),
    ];
    let d = dag(&args, true);
    assert_eq!(uniform(&d, "base_color"), Some(Value::Color3([0.1, 0.2, 0.3])));
    assert_eq!(uniform(&d, "roughness"), Some(Value::Vector2([0.2, 0.3])));
    assert_eq!(uniform(&d, "ior"), Some(Value::Float(1.3)));
    assert_eq!(uniform(&d, "thin"), Some(Value::Boolean(true)));
    assert_eq!(field(&d, "c1_color"), Held::Uniform("base_color".into()));
    assert_eq!(field(&d, "c0_ior"), Held::Uniform("ior".into()));
    // Enums stay constants (they choose code), as do strings.
    assert_eq!(field(&d, "opacity"), Held::Const(Value::Float(0.5)));
    let lacquer = dag(&[arg("preset", MdlArgValue::String("lacquer".into()))], true);
    assert_eq!(field(&lacquer, "c0_ior"), Held::Const(Value::Float(1.8)));
}

#[test]
fn a_texture_argument_takes_the_texture_branch() {
    for uniforms in [false, true] {
        let plain = dag(&[], uniforms);
        assert_eq!(samples(&plain), 0);
        let file = "textures/oak floor.png";
        let textured = |colorspace: Option<&str>| {
            let d = dag(&[arg("base_texture", texture(file, colorspace))], uniforms);
            assert_eq!(d.textures.len(), 1, "{:?}", d.textures);
            assert_eq!(d.textures[0].file, file);
            assert_eq!(samples(&d), 1, "{}", d.dump());
            assert_eq!(field(&d, "c1_color"), Held::Computed);
            d.textures[0].colorspace.clone()
        };
        assert_eq!(textured(Some("sRGB")).as_deref(), Some("srgb_texture"));
        // `raw` is `::tex::gamma_linear`: the texels as stored.
        assert_eq!(textured(Some("raw")), None);
        // `auto` (or none) is `::tex::gamma_default`: sRGB for an 8-bit file.
        assert_eq!(textured(Some("auto")).as_deref(), Some("srgb_texture"));
        assert_eq!(textured(None).as_deref(), Some("srgb_texture"));
    }
}

#[test]
fn an_int_selects_an_enum_value_by_its_value() {
    let opacity = |i: i32| field(&dag(&[arg("finish", MdlArgValue::Int(i))], false), "opacity");
    assert_eq!(opacity(0), Held::Const(Value::Float(1.0)));
    assert_eq!(opacity(3), Held::Const(Value::Float(0.5)));
    assert_eq!(opacity(7), Held::Const(Value::Float(0.25)));
    for out_of_range in [1, -1, 8] {
        let e = error(&[arg("finish", MdlArgValue::Int(out_of_range))]);
        assert!(e.contains("finish") && e.contains(&out_of_range.to_string()), "{e}");
    }
}

#[test]
fn an_unknown_name_is_an_error() {
    let e = error(&[arg("no_such_input", floats(&[1.0]))]);
    assert!(e.contains("no_such_input"), "{e}");
}

#[test]
fn a_type_mismatch_names_the_parameter_and_both_types() {
    for (name, value, given, declared) in [
        ("base_color", floats(&[1.0]), "float", "color"),
        ("roughness", floats(&[0.1, 0.2, 0.3]), "float3", "float2"),
        ("ior", floats(&[]), "0 floats", "float"),
        ("ior", MdlArgValue::Bool(true), "bool", "float"),
        ("thin", MdlArgValue::Int(1), "int", "bool"),
        ("preset", floats(&[1.0]), "float", "string"),
        ("base_texture", MdlArgValue::String("oak.png".into()), "string", "texture_2d"),
        ("ior", texture("oak.png", None), "texture_2d", "float"),
        ("finish", MdlArgValue::String("finish_gloss".into()), "string", "finish_mode"),
    ] {
        let e = error(&[arg(name, value)]);
        assert!(e.contains(name) && e.contains(given) && e.contains(declared), "{name}, {given}, {declared}: {e}");
    }
}

#[test]
fn int_widens_to_float_as_mdl_converts_it() {
    let d = dag(&[arg("ior", MdlArgValue::Int(2))], false);
    assert_eq!(field(&d, "c0_ior"), Held::Const(Value::Float(2.0)));
}

#[test]
fn modules_take_typed_arguments_as_to_ir_with_args_does() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/args.mdl").to_string_lossy().to_string();
    let modules = Modules::new(Resolver::default()).unwrap();
    let options = IrOptions::with_models(&LOBE_MODELS);
    let args = [arg("base_texture", texture("oak.png", Some("sRGB"))), arg("finish", MdlArgValue::Int(3))];
    let got = optimize(&modules.to_ir_with_args(&path, None, &args, &options).unwrap().dag);
    let expected = dag(&args, true);
    assert_eq!(got.dump(), expected.dump());
    assert_eq!(got.textures, expected.textures);
    let unknown = [arg("no_such_input", MdlArgValue::Bool(true))];
    let e = modules.to_ir_with_args(&path, None, &unknown, &options).unwrap_err().to_string();
    assert!(e.contains("no_such_input"), "{e}");
}

#[test]
fn double_parameters_take_floats() {
    let text = "mdl 1.7;\nimport ::df::*;\nexport material d(double w = 0.5, double3 t = double3(1.0)) = material(\
                surface: material_surface(scattering: df::diffuse_reflection_bsdf(tint: color(float3(t) * float(w)))));\n";
    let options = IrOptions { values_as_uniforms: false, ..IrOptions::with_models(&LOBE_MODELS) };
    let args = [arg("w", floats(&[0.25])), arg("t", floats(&[0.4, 0.8, 1.0]))];
    let ir = to_ir_with_args(text, None, None, &args, &options, &Resolver::default()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(field(&optimize(&ir.dag), "c0_color"), Held::Const(Value::Color3([0.1, 0.2, 0.25])));
}
