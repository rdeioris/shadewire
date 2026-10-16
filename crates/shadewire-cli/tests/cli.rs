use std::path::PathBuf;
use std::process::Command;

const EXAMPLE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../vendor/materialx/resources/Materials/Examples/StandardSurface/standard_surface_default.mtlx"
);

const EXAMPLE_MARBLE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../vendor/materialx/resources/Materials/Examples/StandardSurface/standard_surface_marble_solid.mtlx"
);

/// A material that uses a library node defined by a nodegraph (`place2d`); the marble has only leaf ops.
const EXAMPLE_COMPOUNDS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../vendor/materialx/resources/Materials/Examples/StandardSurface/standard_surface_greysphere_calibration.mtlx"
);

const FIXTURE_PRINCIPLED: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../blender/fixtures/graphs/principled_defaults.json");

const FIXTURE_UNSUPPORTED: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../blender/fixtures/graphs/unsupported.json");

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("shadewire-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_shadewire")).args(args).output().unwrap()
}

#[test]
fn hlsl_with_a_reflection_file() {
    let json = scratch("hlsl.json");
    let out = run(&["compile", EXAMPLE, "--lang", "hlsl", "--reflection", json.to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    assert_eq!(value["model"], "standard_surface");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("StandardSurface shadewire_eval_surface() {"), "{stdout}");
    assert!(stdout.contains("ConstantBuffer<MaterialUniforms> shadewire_u : register(b0, space2);"), "{stdout}");
}

#[test]
fn wgsl_goes_to_the_output_file_as_generated() {
    let wgsl = scratch("out.wgsl");
    let out = run(&["compile", EXAMPLE, "-o", wgsl.to_str().unwrap(), "--constants"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = std::fs::read_to_string(&wgsl).unwrap();
    assert!(text.contains("fn shadewire_eval_surface() -> StandardSurface"));
    assert!(out.stderr.is_empty());
}

/// Without uniforms the texture takes binding 0; `--reserve-uniform-binding` keeps 0 for the uniform block.
#[test]
fn reserve_uniform_binding_moves_the_textures_past_the_uniform_slot() {
    let mtlx = scratch("textured.mtlx");
    std::fs::write(
        &mtlx,
        r#"<materialx version="1.39">
  <image name="img" type="color3">
    <input name="file" type="filename" value="a.png" />
  </image>
  <surface_unlit name="SU" type="surfaceshader">
    <input name="emission_color" type="color3" nodename="img" />
  </surface_unlit>
  <surfacematerial name="M" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="SU" />
  </surfacematerial>
</materialx>"#,
    )
    .unwrap();
    let texture_binding = |extra: &[&str], name: &str| {
        let json = scratch(name);
        let mut args = vec!["compile", mtlx.to_str().unwrap(), "--constants", "--reflection", json.to_str().unwrap()];
        args.extend_from_slice(extra);
        let out = run(&args);
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
        assert!(value["uniform_binding"].is_null(), "{value}");
        value["textures"][0]["binding"].as_u64().unwrap()
    };
    assert_eq!(texture_binding(&[], "plain.json"), 0);
    assert_eq!(texture_binding(&["--reserve-uniform-binding"], "reserved.json"), 1);
}

#[test]
fn an_error_is_one_message_and_exit_one() {
    let out = run(&["compile", EXAMPLE, "--model", "gltf_pbr"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no translation"));
    let out = run(&["compile", "does-not-exist.mtlx"]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn every_language_with_its_version() {
    for (lang, extra, expect) in [
        ("glsl", &[][..], "#version 420 core"),
        ("glsl", &["--glsl-version", "330"][..], "#version 330 core"),
        ("glsl-es", &[][..], "#version 300 es"),
        ("msl", &[][..], "#include <metal_stdlib>"),
    ] {
        let mut args = vec!["compile", EXAMPLE, "--lang", lang];
        args.extend_from_slice(extra);
        let out = run(&args);
        assert!(out.status.success(), "{lang}: {}", String::from_utf8_lossy(&out.stderr));
        assert!(String::from_utf8_lossy(&out.stdout).starts_with(expect), "{lang} {extra:?}");
    }
}

#[test]
fn ir_prints_the_dag_before_and_after_the_optimizer() {
    let optimized = run(&["ir", EXAMPLE]);
    assert!(optimized.status.success(), "{}", String::from_utf8_lossy(&optimized.stderr));
    let optimized = String::from_utf8_lossy(&optimized.stdout).to_string();
    assert!(optimized.starts_with("model standard_surface\n"), "{optimized}");
    assert!(optimized.contains("base_color = %"), "{optimized}");
    let raw = run(&["ir", EXAMPLE, "--no-optimize"]);
    let raw = String::from_utf8_lossy(&raw.stdout).to_string();
    assert!(raw.lines().count() > optimized.lines().count(), "the optimizer removed nothing");
}

/// `--lobes` opens a named model into its closure tree on every back end, which then writes MaterialX's lobes.
#[test]
fn lobes_write_the_closure_tree_on_every_back_end() {
    for lang in ["wgsl", "glsl", "glsl-es", "hlsl", "msl", "gdshader"] {
        let json = scratch(&format!("lobes-{lang}.json"));
        let mapped = run(&["compile", EXAMPLE, "--lang", lang]);
        let lobes = run(&["compile", EXAMPLE, "--lang", lang, "--lobes", "--reflection", json.to_str().unwrap()]);
        assert!(lobes.status.success(), "{lang}: {}", String::from_utf8_lossy(&lobes.stderr));
        let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
        assert_eq!(value["model"], "surface", "{lang}");
        let (mapped, lobes) = (String::from_utf8_lossy(&mapped.stdout), String::from_utf8_lossy(&lobes.stdout));
        assert!(!mapped.contains("shadewire_layer_bsdf"), "{lang}: {mapped}");
        assert!(lobes.contains("shadewire_layer_bsdf"), "{lang}: {lobes}");
    }
}

#[test]
fn lobes_and_model_conflict() {
    for command in ["compile", "ir"] {
        let out = run(&[command, EXAMPLE, "--lobes", "--model", "gltf_pbr"]);
        assert!(!out.status.success(), "{command}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("cannot be used with"), "{command}");
    }
}

#[test]
fn ir_with_lobes_is_a_closure_tree() {
    let mapped = run(&["ir", EXAMPLE]);
    let lobes = run(&["ir", EXAMPLE, "--lobes"]);
    assert!(lobes.status.success(), "{}", String::from_utf8_lossy(&lobes.stderr));
    assert_ne!(mapped.stdout, lobes.stdout);
}

#[test]
fn gdshader_flags_need_gdshader() {
    for flag in [&["--transparency", "blend"][..], &["--emission-scale", "2"][..]] {
        let out = run(&[&["compile", EXAMPLE, "--lang", "wgsl"][..], flag].concat());
        assert!(!out.status.success(), "{flag:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("applies to --lang gdshader only"), "{flag:?}");
        let out = run(&[&["compile", EXAMPLE, "--lang", "gdshader"][..], flag].concat());
        assert!(out.status.success(), "{flag:?}: {}", String::from_utf8_lossy(&out.stderr));
    }
}

#[test]
fn mtlx_writes_a_document_that_reads_again() {
    let out = run(&["mtlx", EXAMPLE_MARBLE]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.starts_with(
            "<?xml version=\"1.0\"?>
<materialx version=\"1.39\">"
        ),
        "{text}"
    );
    assert!(text.contains("<standard_surface "), "{text}");
    let again = scratch("again.mtlx");
    std::fs::write(&again, &text).unwrap();
    let listed = run(&["list", again.to_str().unwrap()]);
    assert!(listed.status.success());
}

#[test]
fn mtlx_inline_and_optimize_expand_library_nodes() {
    let kept = String::from_utf8(run(&["mtlx", EXAMPLE_COMPOUNDS]).stdout).unwrap();
    let inlined = String::from_utf8(run(&["mtlx", EXAMPLE_COMPOUNDS, "--inline"]).stdout).unwrap();
    let optimized = String::from_utf8(run(&["mtlx", EXAMPLE_COMPOUNDS, "--optimize"]).stdout).unwrap();
    assert!(kept.len() < inlined.len(), "kept {} inlined {}", kept.len(), inlined.len());
    assert!(kept.contains("nodedef=\"ND_place2d_vector2\""), "{kept}");
    assert!(!inlined.contains("nodedef=\"ND_"), "{inlined}");
    assert!(!optimized.contains("nodedef=\"ND_"), "{optimized}");
    let out = scratch("compounds_out.mtlx");
    let written = run(&["mtlx", EXAMPLE_COMPOUNDS, "-o", out.to_str().unwrap()]);
    assert!(written.status.success() && written.stdout.is_empty());
    assert_eq!(std::fs::read_to_string(out).unwrap(), kept);
}

#[test]
fn a_blender_graph_is_read_by_extension_or_by_format() {
    let listed = run(&["list", FIXTURE_PRINCIPLED]);
    assert!(listed.status.success(), "{}", String::from_utf8_lossy(&listed.stderr));
    assert_eq!(String::from_utf8_lossy(&listed.stdout).trim(), "principled_defaults");
    let ir = run(&["ir", FIXTURE_PRINCIPLED]);
    assert!(ir.status.success());
    assert!(String::from_utf8_lossy(&ir.stdout).starts_with("model open_pbr_surface\n"));
    let compiled = run(&["compile", FIXTURE_PRINCIPLED, "--lang", "wgsl"]);
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    assert!(String::from_utf8_lossy(&compiled.stdout).contains("fn shadewire_eval_surface() -> OpenPbrSurface"));
    let mtlx = run(&["mtlx", FIXTURE_PRINCIPLED]);
    assert!(mtlx.status.success());
    assert!(String::from_utf8_lossy(&mtlx.stdout).contains("<open_pbr_surface "));
    let renamed = scratch("graph.txt");
    std::fs::copy(FIXTURE_PRINCIPLED, &renamed).unwrap();
    assert!(!run(&["list", renamed.to_str().unwrap()]).status.success(), "a .txt is MaterialX by default");
    assert!(run(&["list", renamed.to_str().unwrap(), "--format", "blender"]).status.success());
}

#[test]
fn blender_warnings_go_to_stderr() {
    let out = run(&["ir", FIXTURE_UNSUPPORTED]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("not lowered"), "{stderr}");
    assert!(stderr.contains("ShaderNodeLightPath"), "{stderr}");
}

const MDL_MIXES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../shadewire-mdl/tests/fixtures/mixes.mdl");

const PORCELAIN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../shadewire-mtlx/tests/fixtures/porcelain.mtlx");

#[test]
fn mdl_modules_list_and_compile_with_lobes() {
    let out = run(&["list", MDL_MIXES]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    for name in ["normalized", "clamped", "unbounded", "color_normalized"] {
        assert!(stdout.lines().any(|l| l == name), "{stdout}");
    }
    let out = run(&["compile", MDL_MIXES, "--material", "normalized", "--lobes", "--lang", "wgsl"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let out = run(&["compile", MDL_MIXES, "--material", "normalized"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no translation from surface"));
}

#[test]
fn lang_mdl_writes_a_module() {
    let mdl = scratch("porcelain.mdl");
    let out = run(&["compile", PORCELAIN, "--lang", "mdl", "-o", mdl.to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(std::fs::read_to_string(&mdl).unwrap().starts_with("mdl 1.7;"));
    let out = run(&["compile", EXAMPLE, "--lang", "mdl", "--model", "standard_surface"]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("no translation"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn mdl_path_resolves_imports() {
    let dir = scratch("mdl-lib");
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::write(dir.join("lib/tints.mdl"), "mdl 1.7;\nexport color red() = color(1.0, 0.0, 0.0);\n").unwrap();
    let main = scratch("uses_lib.mdl");
    let text = "mdl 1.7;\nimport ::df::*;\nimport ::lib::tints::*;\n\
                export material m() = material(surface: material_surface(scattering: df::diffuse_reflection_bsdf(tint: lib::tints::red())));\n";
    std::fs::write(&main, text).unwrap();
    let out = run(&["ir", main.to_str().unwrap(), "--lobes", "--mdl-path", dir.to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let out = run(&["ir", main.to_str().unwrap(), "--lobes"]);
    assert!(!out.status.success());
}
