//! Acceptance: every material of the upstream Examples compiles, links against the
//! reference host and translates to every naga back end, or is allow-listed with a
//! reason the spec puts out of phase 1.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use naga::back::{glsl, hlsl};
use shadewire::{CompileOptions, Error, Target, compile_mtlx, link, reference_host, translate};
use shadewire_mtlx::include::resolve_includes;
use shadewire_mtlx::parse::parse_str;

const FOLDERS: [&str; 3] = ["StandardSurface", "OpenPbr", "GltfPbr"];
const ALLOWLIST: &str = include_str!("examples_allowlist.txt");

/// Reasons an Examples material may fail in phase 1; anything else is a bug.
const OUT_OF_PHASE: [&str; 4] = ["bsdf graph", "translation", "matrix", "displacement"];

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/materialx/resources/Materials/Examples")
}

fn targets() -> [Target; 4] {
    [
        Target::Hlsl { shader_model: hlsl::ShaderModel::V6_0 },
        Target::Glsl { version: glsl::Version::Embedded { version: 300, is_webgl: false } },
        Target::Msl { version: (2, 1) },
        Target::SpirV,
    ]
}

/// The `surfacematerial` names of a document, includes resolved.
fn materials(text: &str, path: &Path) -> Vec<String> {
    let name = path.display().to_string();
    let root = parse_str(text, Some(&name)).unwrap();
    let root = resolve_includes(root, path.parent(), Some(&name)).unwrap();
    root.children.iter().filter(|c| c.category == "surfacematerial").map(|c| c.name().to_string()).collect()
}

/// The variant name of an error, as the allow-list spells it.
fn variant(error: &Error) -> String {
    let debug = format!("{error:?}");
    debug.split(|c: char| !c.is_alphanumeric()).next().unwrap_or_default().to_string()
}

/// Compile, link and translate one material; the first failure wins.
fn check(text: &str, path: &Path, material: &str) -> Result<(), Error> {
    let compiled = compile_mtlx(text, Some(path), Some(material), &CompileOptions::default())?;
    let model = shadewire_ir::model_struct_name(&compiled.reflection.model);
    let linked = link(&compiled.wgsl, &reference_host::prelude(&compiled.reflection, &model))?;
    for target in targets() {
        translate(&linked, target)?;
    }
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
        let mut files: Vec<PathBuf> = std::fs::read_dir(examples().join(folder))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "mtlx"))
            .collect();
        files.sort();
        for path in files {
            let text = std::fs::read_to_string(&path).unwrap();
            let file = format!("{folder}/{}", path.file_name().unwrap().to_string_lossy());
            for material in materials(&text, &path) {
                total += 1;
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
    }
    for key in allowed.difference(&used) {
        if !failures.iter().any(|f| f.contains(key.as_str())) {
            failures.push(format!("stale allow-list entry: {key}"));
        }
    }
    println!("{total} materials, {compiled} compiled, {} allow-listed", used.len());
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}
