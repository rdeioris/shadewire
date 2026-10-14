//! Acceptance: every material of the upstream Examples compiles with every back end, and the code validates (WGSL
//! in naga; GLSL in glslangValidator, HLSL in dxc and MSL in Apple's Metal compiler when installed), or is allow-listed with a reason the spec puts
//! out of phase 1.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use shadewire::Error;
use shadewire_mtlx::include::{Resolver, resolve_includes};
use shadewire_nodes::parse::parse_str;
mod common;

use common::compile_mtlx;
use shadewire::shader::CompileOptions;
use shadewire_nodes::IrOptions;

const FOLDERS: [&str; 3] = ["StandardSurface", "OpenPbr", "GltfPbr"];
const ALLOWLIST: &str = include_str!("examples_allowlist.txt");

/// Reasons an Examples material may fail in phase 1; anything else is a bug.
const OUT_OF_PHASE: [&str; 4] = ["bsdf graph", "translation", "matrix", "displacement"];

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/materialx/resources/Materials/Examples")
}

/// The `surfacematerial` names of a document, includes resolved.
fn materials(text: &str, path: &Path) -> Result<Vec<String>, Error> {
    let name = path.display().to_string();
    let root = parse_str(text, Some(&name))?;
    let root = resolve_includes(root, Some(&name), &Resolver::default())?;
    Ok(root.children.iter().filter(|c| c.category == "surfacematerial").map(|c| c.name().to_string()).collect())
}

/// The variant name of an error, as the allow-list spells it.
fn variant(error: &Error) -> String {
    let debug = format!("{error:?}");
    debug.split(|c: char| !c.is_alphanumeric()).next().unwrap_or_default().to_string()
}

/// Compile one material with every back end and validate the code; the first failure wins. A validator's
/// failure is a bug, not an allow-listed error, so it panics.
fn check(text: &str, path: &Path, material: &str) -> Result<(), Error> {
    let options = CompileOptions::default();
    let compiled = compile_mtlx(text, Some(path), Some(material), &IrOptions::default(), &options)?;
    let name = format!("{}_{material}", path.file_stem().unwrap_or_default().to_string_lossy());
    let fail = |language: &str, e: String| -> ! { panic!("{name}: the {language} does not validate:\n{e}") };
    shadewire_wgsl::validate::check(&compiled.code, &compiled.reflection).unwrap_or_else(|e| fail("WGSL", e));
    let ir = shadewire_mtlx::to_ir(text, path.to_str(), Some(material), &IrOptions::default(), &Resolver::default())?;
    let version = shadewire_glsl::Version::Desktop(420);
    let glsl = shadewire_glsl::compile(&ir.dag, &ir.source, &options, version)?;
    shadewire_glsl::validate::check(&name, &glsl.code, &glsl.reflection, version).unwrap_or_else(|e| fail("GLSL", e));
    let hlsl = shadewire_hlsl::compile(&ir.dag, &ir.source, &options)?;
    shadewire_hlsl::validate::check(&name, &hlsl.code, &hlsl.reflection).unwrap_or_else(|e| fail("HLSL", e));
    let msl = shadewire_msl::compile(&ir.dag, &ir.source, &options)?;
    shadewire_msl::validate::check(&name, &msl.code, &msl.reflection).unwrap_or_else(|e| fail("MSL", e));
    Ok(())
}

/// `file|material|variant` keys of the allow-list, each checked for an out-of-phase reason.
fn allowlist() -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for line in ALLOWLIST.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let parts: Vec<&str> = line.split('|').collect();
        assert_eq!(parts.len(), 4, "allow-list line needs file|material|variant|reason: {line}");
        let reason = parts[3].to_lowercase();
        assert!(OUT_OF_PHASE.iter().any(|r| reason.contains(r)), "not an out-of-phase reason: {line}");
        keys.insert(format!("{}|{}|{}", parts[0], parts[1], parts[2]));
    }
    keys
}

#[test]
fn every_example_compiles_or_is_allow_listed() {
    let allowed = allowlist();
    let mut used = BTreeSet::new();
    let mut failures = Vec::new();
    let (mut total, mut compiled) = (0, 0);
    for folder in FOLDERS {
        let dir = examples().join(folder);
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .map(|e| e.unwrap_or_else(|e| panic!("{}: {e}", dir.display())).path())
            .filter(|p| p.extension().is_some_and(|x| x == "mtlx"))
            .collect();
        files.sort();
        let mut in_folder = 0;
        for path in files {
            let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let file = format!("{folder}/{}", path.file_name().unwrap_or_default().to_string_lossy());
            let names = match materials(&text, &path) {
                Ok(names) => names,
                Err(error) => {
                    failures.push(format!("{file}: cannot list its materials: {error}"));
                    continue;
                }
            };
            for material in names {
                total += 1;
                in_folder += 1;
                match check(&text, &path, &material) {
                    Ok(()) => {
                        compiled += 1;
                        let stale: Vec<&String> =
                            allowed.iter().filter(|k| k.starts_with(&format!("{file}|{material}|"))).collect();
                        if !stale.is_empty() {
                            failures.push(format!("stale allow-list entry, now compiles: {stale:?}"));
                        }
                    }
                    Err(error) => {
                        let key = format!("{file}|{material}|{}", variant(&error));
                        if allowed.contains(&key) {
                            used.insert(key);
                        } else {
                            failures.push(format!("{key}: {error}"));
                        }
                    }
                }
            }
        }
        if in_folder == 0 {
            failures.push(format!("{folder}: no material found"));
        }
    }
    for key in allowed.difference(&used) {
        if !failures.iter().any(|f| f.contains(key.as_str())) {
            failures.push(format!("stale allow-list entry: {key}"));
        }
    }
    println!("{total} materials, {compiled} compiled, {} allow-listed", used.len());
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}
