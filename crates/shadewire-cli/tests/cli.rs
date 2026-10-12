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
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("reference host"), "{stderr}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("sw_reference_main"));
}

#[test]
fn wgsl_goes_to_the_output_file_as_generated() {
    let wgsl = scratch("out.wgsl");
    let out = run(&["compile", EXAMPLE, "-o", wgsl.to_str().unwrap(), "--constants"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = std::fs::read_to_string(&wgsl).unwrap();
    assert!(text.contains("fn sw_eval_surface() -> StandardSurface"));
    assert!(!text.contains("sw_reference_main"));
    assert!(out.stderr.is_empty());
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
fn the_note_names_the_language_as_typed() {
    let out = run(&["compile", EXAMPLE, "--lang", "glsl-es"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("note: glsl-es output links the reference host"), "{stderr}");
}

#[test]
fn no_reflection_file_when_the_shader_fails() {
    let json = scratch("failed.json");
    let _ = std::fs::remove_file(&json);
    let out = run(&["compile", EXAMPLE, "--lang", "spirv", "--reflection", json.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("give -o"));
    assert!(!json.exists(), "the reflection was written for a failed compile");
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
