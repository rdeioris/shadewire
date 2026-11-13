use shadewire::optimize::optimize;
use shadewire::{LOBE_MODELS, Value};

use crate::{IrOptions, MaterialIr, Resolver, to_ir};

pub(crate) fn lower(text: &str) -> MaterialIr {
    let options = IrOptions::with_models(&LOBE_MODELS);
    to_ir(text, None, None, &options, &Resolver::default()).unwrap_or_else(|e| panic!("{e}"))
}

pub(crate) fn lower_constants(text: &str) -> MaterialIr {
    let mut options = IrOptions::with_models(&LOBE_MODELS);
    options.values_as_uniforms = false;
    to_ir(text, None, None, &options, &Resolver::default()).unwrap_or_else(|e| panic!("{e}"))
}

/// The value of surface field `name` when it folds to a constant.
pub(crate) fn field_value(ir: &MaterialIr, name: &str) -> Value {
    let dag = optimize(&ir.dag);
    let field =
        dag.surface.fields.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("no field {name}: {}", dag.dump()));
    match &dag.node(field.node).kind {
        shadewire::NodeKind::Const(v) => v.clone(),
        other => panic!("{name} is {other:?}\n{}", dag.dump()),
    }
}

/// A material whose diffuse tint is `expr` (a color), with `prelude` before it.
fn tint_of(prelude: &str, expr: &str) -> Value {
    let text = format!(
        "mdl 1.7;\nimport ::df::*;\nimport ::math::*;\nimport ::state::*;\n{prelude}\nexport material m() = material(surface: material_surface(scattering: df::diffuse_reflection_bsdf(tint: {expr})));\n"
    );
    let ir = lower_constants(&text);
    field_value(&ir, "c0_color")
}

#[test]
fn a_diffuse_material_is_one_lobe() {
    let ir = lower(
        "mdl 1.7;\nimport ::df::*;\nexport material m(color tint = color(0.5, 0.25, 1.0)) = material(surface: material_surface(scattering: df::diffuse_reflection_bsdf(tint: tint)));\n",
    );
    let closure = ir.dag.surface.closure.as_ref().unwrap();
    assert_eq!(
        closure.dump().lines().next().unwrap(),
        "c0 = oren_nayar_diffuse_bsdf(color=c0_color, energy_compensation=c0_energy_compensation, normal=c0_normal, roughness=c0_roughness, weight=c0_weight)"
    );
    let uniform =
        ir.dag.nodes.iter().find(|n| matches!(&n.kind, shadewire::NodeKind::Uniform { name, .. } if name == "tint"));
    assert!(uniform.is_some(), "{}", ir.dag.dump());
}

#[test]
fn arithmetic_folds() {
    assert_eq!(tint_of("", "color(0.25) * 2.0 + color(0.0, 0.5, 1.0)"), Value::Color3([0.5, 1.0, 1.5]));
    assert_eq!(tint_of("", "color(float3(1, 2, 3).y / 4)"), Value::Color3([0.5; 3]));
    assert_eq!(tint_of("", "color(math::lerp(0.0, 2.0, 0.25))"), Value::Color3([0.5; 3]));
    assert_eq!(tint_of("", "color(float(7 / 2) + float(-7 % 3) * 10.0)"), Value::Color3([-7.0; 3]));
}

#[test]
fn functions_with_control_flow() {
    let prelude = r#"
float f(float x) {
    float y = 0.0;
    for (int i = 0; i < 4; ++i) {
        if (i == 2)
            continue;
        y += x;
    }
    return y;
}
int g(int k) {
    switch (k) {
        case 0: return 10;
        case 1:
        case 2: return 20;
        default: break;
    }
    int n = 0;
    while (n < 5) n++;
    return n;
}
struct pair { float a; float b = 3.0; };
float h(pair p) { return p.a * p.b; }
"#;
    assert_eq!(tint_of(prelude, "color(f(0.25))"), Value::Color3([0.75; 3]));
    assert_eq!(tint_of(prelude, "color(float(g(0) + g(2) + g(7)))"), Value::Color3([35.0; 3]));
    assert_eq!(tint_of(prelude, "color(h(pair(2.0)))"), Value::Color3([6.0; 3]));
}

#[test]
fn branches_on_run_time_values_are_predicated() {
    let ir = lower(
        r#"mdl 1.7;
import ::df::*;
import ::state::*;
float pick(float x) {
    if (x > 0.5)
        return 1.0;
    return 0.25;
}
export material m() = material(surface: material_surface(scattering: df::diffuse_reflection_bsdf(tint: color(pick(state::texture_coordinate(0).x)))));
"#,
    );
    let dag = optimize(&ir.dag);
    let dump = dag.dump();
    assert!(dump.contains("IfEqual") || dump.contains("ifequal") || dump.contains("Greater"), "{dump}");
}

#[test]
fn layers_and_fresnel() {
    let ir = lower(
        r#"mdl 1.7;
import ::df::*;
export material m() = material(surface: material_surface(scattering:
    df::fresnel_layer(ior: 1.5, layer: df::microfacet_ggx_smith_bsdf(roughness_u: 0.2), base: df::diffuse_reflection_bsdf(tint: color(0.8)))));
"#,
    );
    let closure = ir.dag.surface.closure.as_ref().unwrap().dump();
    assert!(closure.contains("dielectric_bsdf("), "{closure}");
    assert!(closure.contains("layer(top=c0, base=c1)"), "{closure}");
}

fn try_lower(text: &str, options: &IrOptions) -> Result<MaterialIr, shadewire::Error> {
    to_ir(text, None, None, options, &Resolver::default())
}

/// A material whose diffuse tint is `tint`, in a module of `version` with `prelude`.
fn diffuse(version: &str, prelude: &str, params: &str, tint: &str) -> String {
    format!(
        "mdl {version};\nimport ::df::*;\nimport ::math::*;\nimport ::state::*;\nimport ::anno::*;\n{prelude}\n\
         export material m({params}) = material(surface: material_surface(scattering: df::diffuse_reflection_bsdf(tint: {tint})));\n"
    )
}

#[test]
fn overloads_follow_the_callers_version() {
    for version in ["1.6", "1.11"] {
        let text = format!(
            "mdl {version};\nimport ::df::*;\nexport material m() = material(surface: material_surface(scattering: df::diffuse_reflection_bsdf(color(0.5), 0.2)));\n"
        );
        let ir = lower_constants(&text);
        assert_eq!(field_value(&ir, "c0_roughness"), Value::Float(0.2), "{version}");
    }
}

#[test]
fn using_imports_and_enum_values() {
    let text = diffuse(
        "1.7",
        "using ::tex import wrap_mode;\ncolor pick(wrap_mode w) { return w == wrap_repeat ? color(0.25) : color(1.0); }",
        "",
        "pick(wrap_repeat)",
    );
    assert_eq!(field_value(&lower_constants(&text), "c0_color"), Value::Color3([0.25; 3]));
}

#[test]
fn relative_imports_resolve_against_the_package() {
    let root = std::env::temp_dir().join("shadewire-mdl-relative");
    std::fs::create_dir_all(root.join("pkg/sub")).unwrap();
    std::fs::write(root.join("pkg/util.mdl"), "mdl 1.7;\nexport color c() = color(0.25);\n").unwrap();
    let main = root.join("pkg/sub/mat.mdl");
    let text = diffuse("1.7", "import ..::util::*;", "", "util::c()");
    std::fs::write(&main, &text).unwrap();
    let resolver = Resolver::new(vec![root.clone()]);
    let mut options = IrOptions::with_models(&LOBE_MODELS);
    options.values_as_uniforms = false;
    let location = main.display().to_string();
    let ir = to_ir(&text, Some(&location), None, &options, &resolver).unwrap();
    assert_eq!(field_value(&ir, "c0_color"), Value::Color3([0.25; 3]));
    let missing = diffuse("1.7", "import .::missing::*;", "", "color(1.0)");
    match to_ir(&missing, Some(&location), None, &options, &resolver) {
        Err(shadewire::Error::Io(message)) => assert!(message.contains("::pkg::sub::missing"), "{message}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn parameters_become_uniforms_with_ui_hints() {
    let text = diffuse(
        "1.7",
        "",
        "float r = 0.3 [[ anno::hard_range(0.0, 1.0), anno::soft_range(0.1, 0.9), anno::in_group(\"Base\", \"Spec\") ]], int n = 3",
        "color(r * float(n))",
    );
    let ir = lower(&text);
    let uniforms: Vec<_> = ir
        .dag
        .nodes
        .iter()
        .filter_map(|n| match &n.kind {
            shadewire::NodeKind::Uniform { name, value } => Some((name.clone(), value.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(uniforms, [("r".to_string(), Value::Float(0.3))]);
    let hints = shadewire::UiHints {
        min: Some(Value::Float(0.0)),
        max: Some(Value::Float(1.0)),
        soft_min: Some(Value::Float(0.1)),
        soft_max: Some(Value::Float(0.9)),
        folder: Some("Base/Spec".to_string()),
    };
    assert_eq!(ir.dag.uniform_ui["r"], hints);
}

#[test]
fn errors_name_what_failed() {
    let options = IrOptions::with_models(&LOBE_MODELS);
    let message = |text: &str| match try_lower(text, &options) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("lowered:\n{text}"),
    };
    assert!(message(&diffuse("1.7", "", "", "nope")).contains("unknown name nope"));
    assert!(message(&diffuse("1.7", "", "color t", "t")).contains("parameter t has no default"));
}

/// A `continue` under a run-time condition leaves the counter known, so the loop still unrolls (OmniUber's flakes).
#[test]
fn loops_with_a_run_time_continue_unroll() {
    let prelude = "float f() { float x = state::texture_coordinate(0).x; float s = 0.0;                    for (int i = 0; i < 3; ++i) { for (int j = 0; j < 3; ++j) { if (x * float(i + j) > 0.2) continue; s += 1.0; } }                    return s; }";
    let ir = lower_constants(&diffuse("1.7", prelude, "", "color(f())"));
    let mut probe = shadewire::testing::Probe(0);
    let fields = shadewire::interpret::eval_surface(&ir.dag, &mut probe, &shadewire::testing::ProbeTextures);
    // Probe 0's texcoord x is 0.15: i + j <= 1 passes, three of the nine.
    let color = &fields.iter().find(|(n, _)| n == "c0_color").unwrap().1;
    assert_eq!(color, &Value::Color3([3.0, 3.0, 3.0]));
}

/// MDL's integer operators at run time, on the hash OmniUber_Automotive's car paint flakes use.
#[test]
fn run_time_integer_operators_are_twos_complement() {
    let prelude = "int hash(int seed, int i) { return (i ^ seed) * 1075385539; }
                   int next(int seed) { seed ^= seed << 13; seed ^= seed >>> 17; seed ^= seed << 5; return seed; }
                   color f() { int a = int(state::texture_coordinate(0).x * 1000.0); int s = next(hash(hash(0, a), -7));                    return color(float(s & 0xFFFF), float(s | 3), float(s >> 9)); }";
    let ir = lower_constants(&diffuse("1.7", prelude, "", "f()"));
    let ops: Vec<shadewire::Op> = ir
        .dag
        .nodes
        .iter()
        .filter_map(|n| match &n.kind {
            shadewire::NodeKind::Op { op, .. } => Some(*op),
            _ => None,
        })
        .collect();
    for op in [
        shadewire::Op::BitwiseXor,
        shadewire::Op::BitwiseAnd,
        shadewire::Op::BitwiseOr,
        shadewire::Op::ShiftLeft,
        shadewire::Op::ShiftRight,
        shadewire::Op::ShiftRightUnsigned,
    ] {
        assert!(
            ops.contains(&op),
            "{op:?} missing:
{}",
            ir.dag.dump()
        );
    }
    let mut probe = shadewire::testing::Probe(0);
    let fields = shadewire::interpret::eval_surface(&ir.dag, &mut probe, &shadewire::testing::ProbeTextures);
    // Probe 0's texcoord x is 0.15: a = 150.
    let hash = |seed: i32, i: i32| (i ^ seed).wrapping_mul(1_075_385_539);
    let mut s = hash(hash(0, 150), -7);
    s ^= s << 13;
    s ^= ((s as u32) >> 17) as i32;
    s ^= s << 5;
    let color = &fields.iter().find(|(n, _)| n == "c0_color").unwrap().1;
    assert_eq!(color, &Value::Color3([(s & 0xFFFF) as f32, (s | 3) as f32, (s >> 9) as f32]));
}

#[test]
fn loops_past_the_limit_and_huge_materials_fail_cleanly() {
    let start = std::time::Instant::now();
    let endless = diffuse(
        "1.7",
        "color f() { float x = 0.0; for (int i = 0; i < 1000000; ++i) x += 0.1; return color(x); }",
        "",
        "f()",
    );
    assert!(matches!(try_lower(&endless, &IrOptions::with_models(&LOBE_MODELS)), Err(shadewire::Error::Limit(_))));
    let big = diffuse(
        "1.7",
        "color f() { float x = 0.0; for (int i = 0; i < 1000; ++i) x += state::texture_coordinate(0).x * float(i); return color(x); }",
        "",
        "f()",
    );
    let mut options = IrOptions::with_models(&LOBE_MODELS);
    options.max_nodes = Some(500);
    let r = try_lower(&big, &options);
    assert!(matches!(r, Err(shadewire::Error::Limit(_))), "{:?}", r.map(|ir| ir.dag.nodes.len()));
    assert!(start.elapsed().as_secs() < 10, "{:?}", start.elapsed());
}

#[test]
fn work_without_end_fails_cleanly() {
    let loops = "float g() { float y = 0.0; for (int i = 0; i < 1000; ++i) for (int j = 0; j < 1000; ++j) \
                 for (int k = 0; k < 1000; ++k) y += 0.1; return y; }";
    let mut calls = String::from("float f0(float x) { return x * 1.01; }\n");
    for k in 1..=40 {
        calls += &format!("float f{k}(float x) {{ return f{p}(x) + f{p}(x * 0.5); }}\n", p = k - 1);
    }
    calls += "float g() { return f40(state::texture_coordinate(0).x); }";
    for (what, prelude) in [("a billion iterations", loops.to_string()), ("2^40 calls", calls)] {
        let start = std::time::Instant::now();
        let text = diffuse("1.7", &prelude, "", "color(g())");
        let r = try_lower(&text, &IrOptions::with_models(&LOBE_MODELS));
        assert!(matches!(&r, Err(shadewire::Error::Limit(m)) if m.contains("steps")), "{what}: {:?}", r.map(|_| ()));
        assert!(start.elapsed().as_secs() < 60, "{what}: {:?}", start.elapsed());
    }
}

#[test]
fn deep_nesting_fails_cleanly_on_both_entry_points() {
    let parens = |n: usize| format!("float g() {{ return {}0.5{}; }}", "(".repeat(n), ")".repeat(n));
    let else_ifs = |n: usize| {
        let chain: String = (0..n).map(|i| format!("if (x == {i}) return {i}.0; else ")).collect();
        format!("float g() {{ int x = int(state::texture_coordinate(0).x * 10.0); {chain}return 0.5; }}")
    };
    let calls = |n: usize| {
        format!("float h(float x) {{ return x; }}\nfloat g() {{ return {}0.5{}; }}", "h(".repeat(n), ")".repeat(n))
    };
    let sum = |n: usize| format!("float g() {{ return 0.0{}; }}", "+0.001".repeat(n));
    let lobes = IrOptions::with_models(&LOBE_MODELS);
    for (what, prelude, fits) in [
        ("100 parentheses", parens(100), true),
        ("150 nested calls", calls(150), true),
        ("2000 terms", sum(2000), true),
        ("1500 else ifs", else_ifs(1500), true),
        ("20000 parentheses", parens(20000), false),
        ("5000 else ifs", else_ifs(5000), false),
        ("50000 terms", sum(50000), false),
    ] {
        let text = diffuse("1.7", &prelude, "", "color(g())");
        let names = crate::material_names(&text, None, &Resolver::default());
        let ir = try_lower(&text, &lobes);
        if fits {
            assert_eq!(names.unwrap_or_else(|e| panic!("{what}: {e}")), ["m"], "{what}");
            ir.unwrap_or_else(|e| panic!("{what}: {e}"));
        } else {
            assert!(matches!(names, Err(shadewire::Error::Limit(_))), "{what}: {names:?}");
            match ir {
                Err(shadewire::Error::Limit(m)) => assert!(m.contains(":6"), "{what}: {m}"),
                other => panic!("{what}: {:?}", other.map(|_| ())),
            }
        }
    }
}

#[test]
fn switch_falls_through() {
    let prelude = "int g(int k) { int n = 0; switch (k) { case 1: n += 1; case 2: n += 10; break; default: n = 100; } return n; }";
    assert_eq!(tint_of(prelude, "color(float(g(1) + g(2) * 1000 + g(5) * 1000000))"), Value::Color3([100010011.0; 3]));
}

#[test]
fn vector_components_assign_through_indices() {
    let prelude = "color f(int k) { float3 v = float3(0.0); v[k] = 1.0; v.y += 2.0; float3x3 m = float3x3(0.0); m[1] = v; return color(m[1]); }";
    assert_eq!(tint_of(prelude, "f(2)"), Value::Color3([0.0, 2.0, 1.0]));
}

#[test]
fn a_component_written_by_a_run_time_index_selects() {
    let text = diffuse(
        "1.7",
        "color f() { float3 v = float3(0.0); int k = int(state::texture_coordinate(0).x * 3.0); v[k] = 1.0; return color(v); }",
        "",
        "f()",
    );
    let ir = lower_constants(&text);
    let mut probe = shadewire::testing::Probe(0);
    let fields = shadewire::interpret::eval_surface(&ir.dag, &mut probe, &shadewire::testing::ProbeTextures);
    // Probe 0's texcoord x is 0.15: k = 0.
    let color = &fields.iter().find(|(n, _)| n == "c0_color").unwrap().1;
    assert_eq!(color, &Value::Color3([1.0, 0.0, 0.0]));
}

#[test]
fn shadewire_noise_functions_read_as_their_ops() {
    let prelude = "float shadewire_perlin_noise_float_3d(float3 p) { int a = int(p.x * 7.0); return float(a ^ (a + 1)); }\n\
                   float3 shadewire_fractal_noise_vec3_2d(float2 p, int octaves, float lacunarity, float diminish) { int a = int(p.x * 7.0); return float3(float(a ^ octaves)); }";
    let text = diffuse(
        "1.7",
        prelude,
        "",
        "color(shadewire_perlin_noise_float_3d(state::position())) + color(shadewire_fractal_noise_vec3_2d(float2(state::position().x), 3, 2.0, 0.5))",
    );
    let ir = lower_constants(&text);
    let ops: Vec<shadewire::Op> = ir
        .dag
        .nodes
        .iter()
        .filter_map(|n| match &n.kind {
            shadewire::NodeKind::Op { op, .. } => Some(*op),
            _ => None,
        })
        .collect();
    assert!(ops.contains(&shadewire::Op::Noise3d), "{}", ir.dag.dump());
    assert!(ops.contains(&shadewire::Op::Fractal2d), "{}", ir.dag.dump());
}

/// A search path folder holding `files` (name, text), fresh for test `name`.
fn search_path(name: &str, files: &[(&str, &str)]) -> Resolver {
    let root = std::env::temp_dir().join(format!("shadewire-mdl-{name}"));
    let _ = std::fs::remove_dir_all(&root);
    for (file, text) in files {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    Resolver::new(vec![root])
}

fn lower_with(text: &str, resolver: &Resolver) -> Result<MaterialIr, shadewire::Error> {
    let mut options = IrOptions::with_models(&LOBE_MODELS);
    options.values_as_uniforms = false;
    to_ir(text, None, None, &options, resolver)
}

#[test]
fn a_search_path_module_reexporting_its_own_name_reexports_the_vendored_one() {
    let resolver = search_path("stub-base", &[("base.mdl", "mdl 1.7;\nexport using ::base import *;\n")]);
    let text = diffuse(
        "1.7",
        "import ::base::*;\ncolor pick(base::mono_mode m) { return m == base::mono_average ? color(0.25) : color(1.0); }",
        "uniform base::mono_mode mode = base::mono_average",
        "pick(mode)",
    );
    let ir = lower_with(&text, &resolver).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(field_value(&ir, "c0_color"), Value::Color3([0.25; 3]));
}

#[test]
fn an_enum_chosen_by_a_uniform_is_fixed_at_its_value() {
    let prelude =
        "enum kind { kind_a, kind_b };\ncolor pick(kind k) { return k == kind_a ? color(0.25) : color(1.0); }";
    for (default, expected) in [("false", 1.0), ("true", 0.25)] {
        let text = diffuse("1.7", prelude, &format!("uniform bool flip = {default}"), "pick(flip ? kind_a : kind_b)");
        let ir = lower(&text);
        assert_eq!(field_value(&ir, "c0_color"), Value::Color3([expected; 3]), "{default}");
        assert_eq!(ir.warnings, ["main::m: a kind chosen by flip is fixed at its value"], "{default}");
    }
    let run_time = diffuse("1.7", prelude, "", "pick(state::position().x > 0.5 ? kind_a : kind_b)");
    let message = try_lower(&run_time, &IrOptions::with_models(&LOBE_MODELS)).unwrap_err().to_string();
    assert!(message.contains("a kind chosen by a value known only at run time"), "{message}");
}

#[test]
fn a_declaration_with_constructor_arguments_copies_a_value_of_its_type() {
    let prelude = "bsdf layer() { bsdf b(df::diffuse_reflection_bsdf(tint: color(0.25))); return b; }\n\
                   color read() { texture_2d t(texture_2d(\"wood.png\")); return tex::lookup_color(t, float2(0.5)); }";
    let text = format!(
        "mdl 1.7;\nimport ::df::*;\nimport ::tex::*;\n{prelude}\n\
         export material m() = material(surface: material_surface(scattering: layer()), ior: read());\n"
    );
    let ir = lower_constants(&text);
    assert_eq!(field_value(&ir, "c0_color"), Value::Color3([0.25; 3]));
    assert!(ir.dag.textures.iter().any(|t| t.file.ends_with("wood.png")), "{:?}", ir.dag.textures);
}
