//! `Modules`: each module parsed once, its materials lowered with arguments in place of parameter defaults, from
//! many threads at once.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use shadewire::optimize::optimize;
use shadewire::{Dag, LOBE_MODELS, NodeKind, Value};
use shadewire_mdl::{ArgValue, Argument, IrOptions, Modules, Resolver, material_names, to_ir};

fn fixtures() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    files.into_iter().filter(|f| f.extension().is_some_and(|e| e == "mdl")).collect()
}

fn omni_like() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/omni_like.mdl").to_string_lossy().to_string()
}

fn lobes() -> IrOptions {
    IrOptions::with_models(&LOBE_MODELS)
}

fn arg(name: &str, value: ArgValue) -> Argument {
    Argument { name: name.to_string(), value }
}

fn lower(modules: &Modules, arguments: &[Argument]) -> Dag {
    let ir = modules.to_ir(&omni_like(), Some("omni_like"), arguments, &lobes()).unwrap();
    optimize(&ir.dag)
}

/// The value of uniform `name`.
fn uniform(dag: &Dag, name: &str) -> Option<Value> {
    dag.nodes.iter().find_map(|n| match &n.kind {
        NodeKind::Uniform { name: n, value } if n == name => Some(value.clone()),
        _ => None,
    })
}

/// The value of surface field `name` when it is a constant.
fn constant_field(dag: &Dag, name: &str) -> Option<Value> {
    let field = dag.surface.fields.iter().find(|f| f.name == name)?;
    match &dag.node(field.node).kind {
        NodeKind::Const(v) => Some(v.clone()),
        _ => None,
    }
}

#[test]
fn modules_lower_as_to_ir_does() {
    let modules = Modules::new(Resolver::default()).unwrap();
    let resolver = Resolver::default();
    for path in fixtures() {
        let location = path.to_string_lossy().to_string();
        let text = std::fs::read_to_string(&path).unwrap();
        let names = material_names(&text, Some(&location), &resolver).unwrap();
        assert_eq!(modules.material_names(&location).unwrap(), names, "{location}");
        for m in &names {
            let expected = to_ir(&text, Some(&location), Some(m), &lobes(), &resolver).unwrap();
            let got = modules.to_ir(&location, Some(m), &[], &lobes()).unwrap();
            let (expected_dag, got_dag) = (optimize(&expected.dag), optimize(&got.dag));
            assert_eq!(got_dag.dump(), expected_dag.dump(), "{location}::{m}");
            assert_eq!(got_dag.textures, expected_dag.textures, "{location}::{m}");
            assert_eq!(got.warnings, expected.warnings, "{location}::{m}");
        }
    }
}

#[test]
fn a_module_name_loads_from_the_search_path() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let modules = Modules::new(Resolver::new(vec![fixtures])).unwrap();
    assert_eq!(modules.material_names("::omni_like").unwrap(), ["omni_like"]);
    let by_name = modules.to_ir("::omni_like", None, &[], &lobes()).unwrap();
    let by_file = Modules::new(Resolver::default()).unwrap().to_ir(&omni_like(), None, &[], &lobes()).unwrap();
    assert_eq!(optimize(&by_name.dag).dump(), optimize(&by_file.dag).dump());
}

#[test]
fn a_value_argument_replaces_the_default() {
    let modules = Modules::new(Resolver::default()).unwrap();
    assert_eq!(uniform(&lower(&modules, &[]), "diffuse_color_constant"), Some(Value::Color3([0.2; 3])));
    let color = arg("diffuse_color_constant", ArgValue::Value(Value::Color3([0.1, 0.2, 0.3])));
    let dag = lower(&modules, &[color]);
    assert_eq!(uniform(&dag, "diffuse_color_constant"), Some(Value::Color3([0.1, 0.2, 0.3])));
    assert!(dag.textures.is_empty(), "{:?}", dag.textures);
}

#[test]
fn a_texture_argument_binds_the_file() {
    let modules = Modules::new(Resolver::default()).unwrap();
    let file = "D:/tex/wood grain@2.png";
    let texture = |colorspace: Option<&str>| {
        let value = ArgValue::Texture { file: file.to_string(), colorspace: colorspace.map(str::to_string) };
        let dag = lower(&modules, &[arg("diffuse_texture", value)]);
        assert_eq!(dag.textures.len(), 1, "{:?}", dag.textures);
        assert_eq!(dag.textures[0].file, file);
        dag.textures[0].colorspace.clone()
    };
    assert_eq!(texture(Some("sRGB")).as_deref(), Some("srgb_texture"));
    // `raw` is `::tex::gamma_linear`: the texels as stored, no colorspace.
    assert_eq!(texture(Some("raw")), None);
    // No colorspace is `::tex::gamma_default`: sRGB for an 8-bit file.
    assert_eq!(texture(None).as_deref(), Some("srgb_texture"));
}

#[test]
fn an_int_argument_selects_an_enum_value() {
    let modules = Modules::new(Resolver::default()).unwrap();
    let default = lower(&modules, &[]);
    // `mono_average`, the default: the material is opaque.
    let average = lower(&modules, &[arg("mode", ArgValue::Int(1))]);
    assert_eq!(average.dump(), default.dump());
    assert_eq!(constant_field(&default, "opacity"), Some(Value::Float(1.0)));
    let luminance = lower(&modules, &[arg("mode", ArgValue::Int(2))]);
    assert_eq!(constant_field(&luminance, "opacity"), Some(Value::Float(0.5)), "{}", luminance.dump());
    let count = lower(&modules, &[arg("count", ArgValue::Int(1))]);
    assert_eq!(count.dump(), default.dump());
    let err = modules.to_ir(&omni_like(), None, &[arg("mode", ArgValue::Int(99))], &lobes()).unwrap_err();
    assert!(err.to_string().contains("mode"), "{err}");
}

#[test]
fn an_unknown_argument_is_a_warning() {
    let modules = Modules::new(Resolver::default()).unwrap();
    let plain = modules.to_ir(&omni_like(), Some("omni_like"), &[], &lobes()).unwrap();
    let unknown = arg("inputs_that_do_not_exist", ArgValue::Value(Value::Float(1.0)));
    let ir = modules.to_ir(&omni_like(), Some("omni_like"), &[unknown], &lobes()).unwrap();
    assert_eq!(ir.warnings, ["omni_like::omni_like: no parameter inputs_that_do_not_exist"]);
    assert_eq!(optimize(&ir.dag).dump(), optimize(&plain.dag).dump());
}

#[test]
fn a_mistyped_argument_is_an_error() {
    let modules = Modules::new(Resolver::default()).unwrap();
    let wrong = arg("diffuse_texture", ArgValue::Value(Value::Float(1.0)));
    let err = modules.to_ir(&omni_like(), Some("omni_like"), &[wrong], &lobes()).unwrap_err();
    assert!(err.to_string().contains("diffuse_texture"), "{err}");
}

#[test]
fn modules_are_shared_across_threads() {
    fn send_and_sync<T: Send + Sync>() {}
    send_and_sync::<Modules>();
    let modules = Modules::new(Resolver::default()).unwrap();
    std::thread::scope(|threads| {
        for t in 0..8 {
            let modules = &modules;
            threads.spawn(move || {
                for i in 0..10 {
                    let color = Value::Color3([t as f32 / 8.0, i as f32 / 10.0, 0.5]);
                    let dag = lower(modules, &[arg("diffuse_color_constant", ArgValue::Value(color.clone()))]);
                    assert_eq!(uniform(&dag, "diffuse_color_constant"), Some(color));
                }
            });
        }
    });
}

#[test]
fn a_loaded_module_lowers_quickly() {
    let modules = Modules::new(Resolver::default()).unwrap();
    lower(&modules, &[]);
    let limit = Duration::from_millis(if cfg!(debug_assertions) { 200 } else { 20 });
    let (mut slowest, mut total) = (Duration::ZERO, Duration::ZERO);
    for i in 0..20 {
        let color = arg("diffuse_color_constant", ArgValue::Value(Value::Color3([i as f32 / 20.0, 0.5, 0.5])));
        let start = Instant::now();
        modules.to_ir(&omni_like(), Some("omni_like"), &[color], &lobes()).unwrap();
        let took = start.elapsed();
        slowest = slowest.max(took);
        total += took;
    }
    eprintln!("20 lowerings: {:?} each on average, {slowest:?} the slowest", total / 20);
    assert!(slowest < limit, "{slowest:?}");
}

/// A module of a material `m` whose `color c` defaults to `default`, after `head` (imports and declarations).
fn material(head: &str, default: &str) -> String {
    format!(
        "mdl 1.7;\nimport ::df::*;\n{head}export material m(color c = {default}) = \
         material(surface: material_surface(scattering: df::diffuse_reflection_bsdf(tint: c)));\n"
    )
}

/// A module exporting `tint()`, `color(value)`, and a material `m` of that tint.
fn library(value: f32) -> String {
    material(&format!("export color tint() = color({value:?});\n"), "tint()")
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn scratch(name: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join("mdl-modules").join(name)
}

/// The default of material `m`'s `c` in `module`.
fn tint(modules: &Modules, module: &Path) -> Option<Value> {
    let ir = modules.to_ir(module.to_str().unwrap(), Some("m"), &[], &lobes()).unwrap();
    uniform(&ir.dag, "c")
}

#[test]
fn same_named_files_in_two_folders_import_their_own_siblings() {
    let root = scratch("same-names");
    for (folder, value) in [("a", 0.1), ("b", 0.9)] {
        write(&root.join(folder).join("Paint_Satin.mdl"), &material("import Paint_Base::*;\n", "Paint_Base::tint()"));
        write(&root.join(folder).join("Paint_Base.mdl"), &library(value));
    }
    let modules = Modules::new(Resolver::default()).unwrap();
    let (a, b) = (root.join("a/Paint_Satin.mdl"), root.join("b/Paint_Satin.mdl"));
    assert_eq!(tint(&modules, &a), Some(Value::Color3([0.1; 3])));
    assert_eq!(tint(&modules, &b), Some(Value::Color3([0.9; 3])));
    assert_eq!(tint(&modules, &a), Some(Value::Color3([0.1; 3])));
}

#[test]
fn a_local_copy_and_the_search_path_module_of_its_name_stay_apart() {
    let root = scratch("local-copy");
    write(&root.join("search/OmniPBR.mdl"), &library(0.3));
    write(&root.join("local/OmniPBR.mdl"), &library(0.7));
    write(&root.join("user/user.mdl"), &material("import ::OmniPBR::*;\n", "OmniPBR::tint()"));
    let (local, user) = (root.join("local/OmniPBR.mdl"), root.join("user/user.mdl"));
    let by_name = |modules: &Modules| {
        let ir = modules.to_ir("::OmniPBR", Some("m"), &[], &lobes()).unwrap();
        uniform(&ir.dag, "c")
    };
    let resolver = Resolver::new(vec![root.join("search")]);
    let modules = Modules::new(resolver.clone()).unwrap();
    assert_eq!(tint(&modules, &local), Some(Value::Color3([0.7; 3])));
    assert_eq!(by_name(&modules), Some(Value::Color3([0.3; 3])));
    assert_eq!(tint(&modules, &user), Some(Value::Color3([0.3; 3])));
    assert_eq!(tint(&modules, &local), Some(Value::Color3([0.7; 3])));
    let modules = Modules::new(resolver).unwrap();
    assert_eq!(by_name(&modules), Some(Value::Color3([0.3; 3])));
    assert_eq!(tint(&modules, &local), Some(Value::Color3([0.7; 3])));
    assert_eq!(by_name(&modules), Some(Value::Color3([0.3; 3])));
}

/// A module of material `base(color tint = color(0.1))` and what `variants` declare after it.
fn variants(declarations: &str) -> String {
    format!(
        "mdl 1.7;\nimport ::df::*;\nexport material base(color tint = color(0.1), float rough = 0.2) = \
         material(surface: material_surface(scattering: df::diffuse_reflection_bsdf(tint: tint, roughness: rough)));\n\
         {declarations}"
    )
}

fn variant_ir(name: &str, text: &str, arguments: &[Argument]) -> shadewire_mdl::MaterialIr {
    let file = scratch(name).join("variants.mdl");
    write(&file, text);
    let modules = Modules::new(Resolver::default()).unwrap();
    modules.to_ir(file.to_str().unwrap(), Some("v"), arguments, &lobes()).unwrap()
}

#[test]
fn a_variant_has_the_parameters_of_its_base_with_its_own_defaults() {
    let text = variants("export material v(*) = base(tint: color(0.5));\n");
    let ir = variant_ir("variant-defaults", &text, &[]);
    assert!(ir.warnings.is_empty(), "{:?}", ir.warnings);
    let dag = optimize(&ir.dag);
    assert_eq!(uniform(&dag, "tint"), Some(Value::Color3([0.5; 3])));
    // What the variant does not set keeps the base's default.
    assert_eq!(uniform(&dag, "rough"), Some(Value::Float(0.2)));
}

#[test]
fn a_callers_argument_wins_over_a_variants() {
    let text = variants("export material v(*) = base(tint: color(0.5));\n");
    let tint = arg("tint", ArgValue::Value(Value::Color3([0.9; 3])));
    let rough = arg("rough", ArgValue::Value(Value::Float(0.7)));
    let ir = variant_ir("variant-arguments", &text, &[tint, rough]);
    assert!(ir.warnings.is_empty(), "{:?}", ir.warnings);
    let dag = optimize(&ir.dag);
    assert_eq!(uniform(&dag, "tint"), Some(Value::Color3([0.9; 3])));
    assert_eq!(uniform(&dag, "rough"), Some(Value::Float(0.7)));
}

#[test]
fn an_unknown_argument_to_a_variant_is_a_warning() {
    let text = variants("export material v(*) = base(tint: color(0.5));\n");
    let ir = variant_ir("variant-unknown", &text, &[arg("nope", ArgValue::Value(Value::Float(1.0)))]);
    assert_eq!(ir.warnings, ["variants::v: no parameter nope"]);
}

#[test]
fn a_variant_of_a_variant_takes_the_outer_defaults_first() {
    let text = variants(
        "export material mid(*) = base(tint: color(0.3), rough: 0.4);\n\
         export material v(*) = mid(tint: color(0.6));\n",
    );
    let dag = optimize(&variant_ir("variant-chain", &text, &[]).dag);
    assert_eq!(uniform(&dag, "tint"), Some(Value::Color3([0.6; 3])));
    assert_eq!(uniform(&dag, "rough"), Some(Value::Float(0.4)));
}
