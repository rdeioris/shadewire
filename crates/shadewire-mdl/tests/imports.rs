//! Relative imports (`.::`, `..::`) resolve against the importing file's package whether the file and the search path
//! are given as absolute or relative paths, and next to the file when it is on no search path.

use std::path::{Component, Path, PathBuf};

use shadewire::optimize::optimize;
use shadewire::{LOBE_MODELS, NodeKind};
use shadewire_mdl::{IrOptions, Modules, Resolver, to_ir};

const UTIL: &str = "mdl 1.7;\nexport color tint() = color(0.2, 0.4, 0.6);\n";

fn module(import: &str) -> String {
    format!(
        "mdl 1.7;\nimport ::df::*;\nimport {import};\n\
         export material m() = material(surface: material_surface(scattering: \
         df::diffuse_reflection_bsdf(tint: util::tint())));\n"
    )
}

/// `to` relative to `from` (both absolute), as `..` and names.
fn relative(from: &Path, to: &Path) -> PathBuf {
    let (from, to): (Vec<Component>, Vec<Component>) = (from.components().collect(), to.components().collect());
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut out = PathBuf::new();
    for _ in common..from.len() {
        out.push("..");
    }
    for c in &to[common..] {
        out.push(c);
    }
    out
}

/// A tree `root/pkg/{util,mat}.mdl`, `root/pkg/sub/deep.mdl`; the root as a path relative to the working directory.
fn tree(name: &str) -> (PathBuf, PathBuf) {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("mdl-imports").join(name);
    std::fs::create_dir_all(root.join("pkg/sub")).unwrap();
    std::fs::write(root.join("pkg/util.mdl"), UTIL).unwrap();
    std::fs::write(root.join("pkg/mat.mdl"), module(".::util::*")).unwrap();
    std::fs::write(root.join("pkg/sub/deep.mdl"), module("..::util::*")).unwrap();
    std::fs::write(root.join("pkg/bad.mdl"), "mdl 1.7;\nexport color tint() = nope();\n").unwrap();
    std::fs::write(root.join("pkg/broken.mdl"), module(".::bad::*").replace("util::", "bad::")).unwrap();
    std::fs::write(root.join("pkg/missing.mdl"), module(".::nope::*").replace("util::tint()", "color(1.0)")).unwrap();
    let cwd = std::env::current_dir().unwrap();
    (root.clone(), relative(&cwd, &root))
}

fn lower(file: &Path, paths: &[PathBuf]) -> Result<(), String> {
    let text = std::fs::read_to_string(file).unwrap();
    let options = IrOptions::with_models(&LOBE_MODELS);
    let location = file.to_str().unwrap();
    to_ir(&text, Some(location), None, &options, &Resolver::new(paths.to_vec())).map(|_| ()).map_err(|e| e.to_string())
}

#[test]
fn relative_imports_resolve_from_relative_paths() {
    let (absolute, relative) = tree("relative");
    let ok = |file: &Path, paths: &[PathBuf], what: &str| {
        lower(file, paths).unwrap_or_else(|e| panic!("{what}: {e}"));
    };
    ok(&relative.join("pkg/mat.mdl"), &[], "a relative file on no search path");
    ok(&relative.join("pkg/mat.mdl"), std::slice::from_ref(&relative), "a relative file under a relative root");
    ok(&relative.join("pkg/mat.mdl"), std::slice::from_ref(&absolute), "a relative file under an absolute root");
    ok(&absolute.join("pkg/mat.mdl"), std::slice::from_ref(&relative), "an absolute file under a relative root");
    ok(&relative.join("pkg/sub/deep.mdl"), std::slice::from_ref(&relative), "..:: under a relative root");
    let missing = lower(&relative.join("pkg/missing.mdl"), &[]).unwrap_err();
    assert!(missing.contains("::nope"), "{missing}");
    // An error in an imported module names its file as a person would write it.
    let broken = lower(&relative.join("pkg/broken.mdl"), &[]).unwrap_err();
    assert!(broken.contains("bad.mdl:2") && !broken.contains(r"\\?\"), "{broken}");
}

const KIT_MAIN: &str = "mdl 1.8;\nimport ::df::*;\nimport ::nvidia::core::*;\n\
    export material m() = material(surface: material_surface(scattering: \
    df::diffuse_reflection_bsdf(tint: nvidia::core::c())));\n";

/// A search path root `kit/mdl` holding `nvidia/core.mdl`, which imports `support` without a prefix, and `support.mdl`
/// where `support_at` (below the root) says; `None` for none.
fn kit(name: &str, support_at: Option<&str>) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("mdl-imports").join(name).join("kit/mdl");
    std::fs::create_dir_all(root.join("nvidia")).unwrap();
    std::fs::write(root.join("nvidia/core.mdl"), "mdl 1.8;\nimport support::*;\nexport color c() = support::s();\n")
        .unwrap();
    if let Some(support_at) = support_at {
        std::fs::write(root.join(support_at), "mdl 1.8;\nexport color s() = color(0.25);\n").unwrap();
    }
    root
}

/// The diffuse tint of `KIT_MAIN` lowered against the search path `root`.
fn kit_tint(root: &Path) -> Result<Vec<f32>, String> {
    let options = IrOptions::with_models(&LOBE_MODELS);
    let ir = to_ir(KIT_MAIN, None, Some("m"), &options, &Resolver::new(vec![root.to_path_buf()]))
        .map_err(|e| e.to_string())?;
    let dag = optimize(&ir.dag);
    let tint = dag.surface.fields.iter().find(|f| f.name == "c0_color").expect("a diffuse tint");
    match &dag.node(tint.node).kind {
        NodeKind::Const(v) => Ok(v.floats().to_vec()),
        other => panic!("the tint did not fold: {other:?}"),
    }
}

#[test]
fn imports_without_a_prefix_look_beside_the_importer_first() {
    let root = kit("beside", Some("nvidia/support.mdl"));
    assert_eq!(kit_tint(&root).unwrap(), vec![0.25; 3]);
}

#[test]
fn imports_without_a_prefix_fall_back_to_the_search_path() {
    let root = kit("search-path", Some("support.mdl"));
    assert_eq!(kit_tint(&root).unwrap(), vec![0.25; 3]);
}

#[test]
fn a_missing_module_names_both_places() {
    let root = kit("missing", None);
    let error = kit_tint(&root).unwrap_err();
    assert!(error.contains("::nvidia::support") && error.contains("::support"), "{error}");
}

/// `root/a/b/glass.mdl` importing `import` as `util`, and `root/a/templates/util.mdl` with `UTIL`'s tint scaled by `scale`.
fn glass(name: &str, import: &str, scale: f32) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("mdl-imports").join(name);
    std::fs::create_dir_all(root.join("a/b")).unwrap();
    std::fs::create_dir_all(root.join("a/templates")).unwrap();
    let util = format!("mdl 1.7;\nexport color tint() = color({:?});\n", 0.5 * scale);
    std::fs::write(root.join("a/templates/util.mdl"), util).unwrap();
    std::fs::write(root.join("a/b/glass.mdl"), module(import)).unwrap();
    root.join("a/b/glass.mdl")
}

/// The diffuse tint of `file`'s material, loaded by file path with an empty search path.
fn glass_tint(modules: &Modules, file: &Path) -> Result<Vec<f32>, String> {
    let options = IrOptions::with_models(&LOBE_MODELS);
    let ir = modules.to_ir(file.to_str().unwrap(), None, &[], &options).map_err(|e| e.to_string())?;
    let dag = optimize(&ir.dag);
    let tint = dag.surface.fields.iter().find(|f| f.name == "c0_color").expect("a diffuse tint");
    match &dag.node(tint.node).kind {
        NodeKind::Const(v) => Ok(v.floats().to_vec()),
        other => panic!("the tint did not fold: {other:?}"),
    }
}

#[test]
fn a_parent_import_from_a_file_off_the_search_path_goes_up_a_folder() {
    let file = glass("parent-off-path", "..::templates::util::*", 1.0);
    let modules = Modules::new(Resolver::default()).unwrap();
    assert_eq!(glass_tint(&modules, &file).unwrap(), vec![0.5; 3]);
}

#[test]
fn off_path_imports_are_kept_by_file() {
    let (one, two) =
        (glass("off-path-one", "..::templates::util::*", 1.0), glass("off-path-two", "..::templates::util::*", 0.5));
    let modules = Modules::new(Resolver::default()).unwrap();
    assert_eq!(glass_tint(&modules, &one).unwrap(), vec![0.5; 3]);
    assert_eq!(glass_tint(&modules, &two).unwrap(), vec![0.25; 3]);
    assert_eq!(glass_tint(&modules, &one).unwrap(), vec![0.5; 3]);
}

#[test]
fn a_parent_import_above_the_file_system_root_is_an_error() {
    let above = format!("{}templates::util::*", "..::".repeat(200));
    let file = glass("above-root", &above, 1.0);
    let modules = Modules::new(Resolver::default()).unwrap();
    let error = glass_tint(&modules, &file).unwrap_err();
    assert!(error.contains("above the file system root") && error.contains("glass.mdl:3"), "{error}");
}

#[test]
fn a_parent_import_of_a_module_that_is_not_there_names_the_folder() {
    let file = glass("not-there", "..::templates::nope::*", 1.0);
    let modules = Modules::new(Resolver::default()).unwrap();
    let error = glass_tint(&modules, &file).unwrap_err();
    assert!(error.contains("no MDL module") && error.contains("nope") && error.contains("glass.mdl:3"), "{error}");
}

#[test]
fn a_module_reached_by_a_parent_import_finds_its_own_siblings() {
    // glass.mdl reaches templates/wrapper.mdl, which imports util beside it without a prefix.
    let glass = glass("siblings", "..::templates::wrapper::*", 1.0);
    let templates = glass.parent().unwrap().parent().unwrap().join("templates");
    std::fs::write(
        templates.join("wrapper.mdl"),
        "mdl 1.7;
import util::*;
export color tint() = util::tint();
",
    )
    .unwrap();
    std::fs::write(&glass, module("..::templates::wrapper::*").replace("util::tint()", "wrapper::tint()")).unwrap();
    let modules = Modules::new(Resolver::default()).unwrap();
    assert_eq!(glass_tint(&modules, &glass).unwrap(), vec![0.5; 3]);
}
