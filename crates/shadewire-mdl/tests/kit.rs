//! Kit's core MDL library (`SHADEWIRE_MDL_KIT` names the folder holding its `Base/` and `mdl/`, which are the search
//! path): every exported material of `Base/*.mdl` lowers to lobes. Skipped without the variable.

use std::path::PathBuf;
use std::time::Instant;

use shadewire::LOBE_MODELS;
use shadewire_mdl::{IrOptions, Modules, Resolver};

#[test]
fn kit_base_materials_lower() {
    let Some(kit) = std::env::var_os("SHADEWIRE_MDL_KIT").map(PathBuf::from) else {
        eprintln!("SHADEWIRE_MDL_KIT not set: skipped");
        return;
    };
    let base = kit.join("Base");
    let modules = Modules::new(Resolver::new(vec![kit.join("mdl"), base.clone()])).unwrap();
    let options = IrOptions::with_models(&LOBE_MODELS);
    let mut files: Vec<PathBuf> = std::fs::read_dir(&base).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    let mut failures = Vec::new();
    let mut count = 0;
    let mut omni_pbr = None;
    for file in files.iter().filter(|f| f.extension().is_some_and(|e| e == "mdl")) {
        let stem = file.file_stem().unwrap().to_string_lossy().to_string();
        let module = format!("::{stem}");
        let names = match modules.material_names(&module) {
            Ok(names) => names,
            Err(e) => {
                failures.push(format!("{stem}: {e}"));
                continue;
            }
        };
        for name in names {
            if let Err(e) = modules.to_ir(&module, Some(&name), &[], &options) {
                failures.push(format!("{stem} {name}: {e}"));
                continue;
            }
            // The first lowering loads nothing new; this one times the lowering alone.
            let start = Instant::now();
            let ir = modules.to_ir(&module, Some(&name), &[], &options).unwrap();
            let elapsed = start.elapsed();
            println!("{stem} {name}: {elapsed:?}, {} warnings", ir.warnings.len());
            for warning in &ir.warnings {
                println!("    {warning}");
            }
            if stem == "OmniPBR" && name == "OmniPBR" {
                omni_pbr = Some(elapsed);
            }
            count += 1;
        }
    }
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
    println!("{count} materials");
    let omni_pbr = omni_pbr.expect("OmniPBR lowers");
    if !cfg!(debug_assertions) {
        assert!(omni_pbr.as_millis() < 100, "OmniPBR lowers in {omni_pbr:?}");
    }
}
