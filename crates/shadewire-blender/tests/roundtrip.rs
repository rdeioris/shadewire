//! The hub's promise: `blender → ir → mtlx → ir` is the same material as `blender → ir`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use shadewire::Error;
use shadewire::testing::assert_same_material;
use shadewire_blender::{IrOptions, material_names, to_ir};
use shadewire_mtlx::{Resolver, WriteOptions, from_ir};

fn fixtures() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../blender/fixtures/graphs");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    files
        .into_iter()
        .filter(|f| f.extension().is_some_and(|e| e == "json"))
        .map(|f| (f.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(&f).unwrap()))
        .collect()
}

#[test]
fn every_fixture_survives_the_trip_through_materialx() {
    let fixtures = fixtures();
    let mut count = 0;
    let mut tripped = BTreeSet::new();
    for (name, text) in &fixtures {
        for material in material_names(text).unwrap() {
            let modes =
                [("models", IrOptions::default()), ("lobes", IrOptions::with_models(&["surface", "surface_unlit"]))];
            for (mode, options) in modes {
                let a = match to_ir(text, Some(&material), &options) {
                    Ok(ir) => ir.dag,
                    // These two make closure trees, which only the `surface` model takes.
                    Err(Error::NoTranslation { .. }) if mode == "models" && (name == "lobes" || name == "math") => {
                        continue;
                    }
                    Err(e) => panic!("{name}/{material}: {e}"),
                };
                for inline in [false, true] {
                    let write = WriteOptions { inline, name: material.clone(), ..WriteOptions::default() };
                    let written = from_ir(&a, &write).unwrap_or_else(|e| panic!("{name}/{material}: writing: {e}"));
                    let b = shadewire_mtlx::to_ir(&written.text, None, None, &options, &Resolver::default())
                        .unwrap_or_else(|e| {
                            panic!("{name}/{material}: reading the written document: {e}\n{}", written.text)
                        })
                        .dag;
                    assert_same_material(&a, &b, &written.text);
                    tripped.insert(name.as_str());
                    count += 1;
                }
            }
        }
    }
    let names: BTreeSet<&str> = fixtures.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(tripped, names, "every fixture makes at least one round trip");
    assert!(count >= 36, "{count} round trips");
}
