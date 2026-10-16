//! Every MaterialX document in the repository builds: the upstream resources, the test fixtures, the oracle
//! materials and the libraries themselves. The builder checks names as XML elements land (unique in their scope,
//! inputs unique in their node), so a document these checks refuse fails here, naming the element.

use std::path::{Path, PathBuf};

use shadewire_mtlx::include::{Resolver, resolve_includes};
use shadewire_nodes::document::build::build;
use shadewire_nodes::parse::parse_str;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        if path.is_dir() {
            if name != "target" && !name.starts_with('.') {
                collect(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "mtlx") {
            out.push(path);
        }
    }
}

fn build_file(path: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let name = path.display().to_string();
    let root = parse_str(&text, Some(&name)).map_err(|e| e.to_string())?;
    let root = resolve_includes(root, Some(&name), &Resolver::default()).map_err(|e| e.to_string())?;
    build(&root, Some(&name)).map(|_| ()).map_err(|e| e.to_string())
}

#[test]
fn every_document_in_the_repository_builds() {
    let mut files = Vec::new();
    for dir in ["vendor/materialx/resources", "crates", "tools"] {
        collect(&repo().join(dir), &mut files);
    }
    files.sort();
    assert!(files.len() > 100, "found only {} documents", files.len());
    let failures: Vec<String> =
        files.iter().filter_map(|f| build_file(f).err().map(|e| format!("{}: {e}", f.display()))).collect();
    assert!(
        failures.is_empty(),
        "{} of {} documents do not build:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n")
    );
    // The embedded standard library is built through the same path; loading it panics on a refused document.
    assert!(!shadewire_nodes::library::standard().documents.is_empty());
}
