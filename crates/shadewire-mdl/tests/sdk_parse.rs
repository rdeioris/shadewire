//! Every `.mdl` file of the MDL SDK's source tree parses (examples, standard modules, MaterialX's MDL library) (`SHADEWIRE_MDL_SDK` names the SDK's source root). Skipped
//! without the variable.

use std::path::{Path, PathBuf};

use shadewire_mdl::parse::parse_module;

fn mdl_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            mdl_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "mdl") {
            out.push(path);
        }
    }
}

#[test]
fn the_sdk_modules_parse() {
    let Some(root) = std::env::var_os("SHADEWIRE_MDL_SDK") else {
        eprintln!("SHADEWIRE_MDL_SDK not set: skipped");
        return;
    };
    let mut files = Vec::new();
    mdl_files(&PathBuf::from(root), &mut files);
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        if let Err(e) = parse_module(&text, Some(&file.display().to_string())) {
            panic!("{e}");
        }
    }
    assert!(files.len() >= 100, "{} files", files.len());
}
