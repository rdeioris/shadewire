//! The MDL SDK's example materials (`SHADEWIRE_MDL_SDK` names the SDK, whose `examples/mdl` is the search path):
//! every material lowers, its WGSL validates, and its MaterialX document reads back. Skipped without the variable.

use std::path::PathBuf;

use shadewire::LOBE_MODELS;
use shadewire::shader::CompileOptions;
use shadewire_mdl::{IrOptions, Resolver, material_names, to_ir};

fn sdk_examples() -> Option<PathBuf> {
    let root = PathBuf::from(std::env::var_os("SHADEWIRE_MDL_SDK")?);
    Some(root.join("examples/mdl"))
}

#[test]
fn the_sdk_examples_lower_validate_and_write_materialx() {
    let Some(search) = sdk_examples() else {
        eprintln!("SHADEWIRE_MDL_SDK not set: skipped");
        return;
    };
    let resolver = Resolver::new(vec![search.clone()]);
    let options = IrOptions::with_models(&LOBE_MODELS);
    let dir = search.join("nvidia/sdk_examples");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    let start = std::time::Instant::now();
    let mut count = 0;
    for file in files.iter().filter(|f| f.extension().is_some_and(|e| e == "mdl")) {
        let text = std::fs::read_to_string(file).unwrap();
        let location = file.display().to_string();
        for name in material_names(&text, Some(&location), &resolver).unwrap() {
            let ir = to_ir(&text, Some(&location), Some(&name), &options, &resolver)
                .unwrap_or_else(|e| panic!("{location} {name}: {e}"));
            println!("{name}: {} warnings", ir.warnings.len());
            let wgsl = shadewire_wgsl::compile(&ir.dag, &ir.source, &CompileOptions::default())
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            shadewire_wgsl::validate::check(&wgsl.code, &wgsl.reflection)
                .unwrap_or_else(|e| panic!("{name}: the WGSL does not validate:\n{e}"));
            let write = shadewire_mtlx::WriteOptions { name: name.clone(), ..Default::default() };
            let written = shadewire_mtlx::from_ir(&ir.dag, &write).unwrap_or_else(|e| panic!("{name}: {e}"));
            shadewire_mtlx::to_ir(&written.text, None, None, &options, &shadewire_mtlx::Resolver::default())
                .unwrap_or_else(|e| panic!("{name}: the MaterialX does not read back: {e}\n{}", written.text));
            count += 1;
        }
    }
    assert!(count > 30, "{count} materials");
    // A release build reads the whole corpus in seconds; a debug build gets more room.
    let budget = if cfg!(debug_assertions) { 600 } else { 120 };
    assert!(start.elapsed().as_secs() < budget, "{:?}", start.elapsed());
}
