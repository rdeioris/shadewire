use std::path::PathBuf;
use std::process::Command;

const EXAMPLE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../vendor/materialx/resources/Materials/Examples/StandardSurface/standard_surface_default.mtlx"
);

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
