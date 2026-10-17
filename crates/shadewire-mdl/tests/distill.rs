//! Distilling closure trees to `open_pbr_surface` (`IrOptions::distill`): every MDL fixture and every vendored
//! MaterialX example distills, and open_pbr_surface's own tree distills back to its fields.

use std::path::{Path, PathBuf};

use shadewire::interpret::eval_surface;
use shadewire::testing::{ProbeTextures, probes};
use shadewire::{Dag, LOBE_MODELS, Value};
use shadewire_mdl::{IrOptions, Resolver, material_names, to_ir};
use shadewire_nodes::surface::distill::distill;

fn distilling() -> IrOptions {
    IrOptions { distill: true, ..IrOptions::with_models(&["open_pbr_surface"]) }
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixtures() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(fixtures_dir()).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    files.into_iter().filter(|f| f.extension().is_some_and(|e| e == "mdl")).collect()
}

/// `(file, material, text)` of every vendored MaterialX example material.
fn examples() -> Vec<(PathBuf, String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/materialx/resources/Materials/Examples");
    let mut out = Vec::new();
    for folder in ["StandardSurface", "OpenPbr", "GltfPbr"] {
        let mut files: Vec<PathBuf> =
            std::fs::read_dir(root.join(folder)).unwrap().map(|e| e.unwrap().path()).collect();
        files.sort();
        for file in files.into_iter().filter(|f| f.extension().is_some_and(|e| e == "mtlx")) {
            let text = std::fs::read_to_string(&file).unwrap();
            let location = file.display().to_string();
            let Ok(names) = shadewire_mtlx::material_names(&text, Some(&location), &Default::default()) else {
                continue;
            };
            for name in names {
                out.push((file.clone(), name, text.clone()));
            }
        }
    }
    out
}

/// The surface's fields at each probe.
fn evaluate(dag: &Dag) -> Vec<Vec<(String, Value)>> {
    probes().into_iter().map(|mut p| eval_surface(dag, &mut p, &ProbeTextures)).collect()
}

#[track_caller]
fn assert_distilled(dag: &Dag, what: &str) {
    assert_eq!(dag.surface.model, "open_pbr_surface", "{what}");
    assert!(dag.surface.closure.is_none(), "{what}");
    for probe in evaluate(dag) {
        for (name, v) in probe {
            assert!(v.floats().iter().all(|x| x.is_finite()), "{what}: {name} = {v:?}");
        }
    }
}

#[test]
fn every_mdl_fixture_distills() {
    let resolver = Resolver::new(vec![fixtures_dir()]);
    let mut count = 0;
    for path in fixtures() {
        let location = path.to_string_lossy().to_string();
        let text = std::fs::read_to_string(&path).unwrap();
        for name in material_names(&text, Some(&location), &resolver).unwrap() {
            let what = format!("{location} {name}");
            let ir = to_ir(&text, Some(&location), Some(&name), &distilling(), &resolver)
                .unwrap_or_else(|e| panic!("{what}: {e}"));
            assert_distilled(&ir.dag, &what);
            count += 1;
        }
    }
    assert!(count >= 14, "{count} materials");
}

#[test]
fn every_materialx_example_distills() {
    let mut count = 0;
    for (file, material, text) in examples() {
        let location = file.display().to_string();
        let what = format!("{location} {material}");
        // A model the host does not accept opens into its lobes and distills; open_pbr_surface stays itself.
        let ir = shadewire_mtlx::to_ir(&text, Some(&location), Some(&material), &distilling(), &Default::default())
            .unwrap_or_else(|e| panic!("{what}: {e}"));
        assert_distilled(&ir.dag, &what);
        // Every example's closure tree, open_pbr_surface's included, distills.
        let lobes = IrOptions::with_models(&LOBE_MODELS);
        let mut dag = shadewire_mtlx::to_ir(&text, Some(&location), Some(&material), &lobes, &Default::default())
            .unwrap_or_else(|e| panic!("{what}: {e}"))
            .dag;
        distill(&mut dag).unwrap_or_else(|e| panic!("{what}: {e}"));
        assert_distilled(&dag, &what);
        count += 1;
    }
    assert!(count >= 54, "{count} materials");
}

/// Fields the closure tree does not carry: the medium under a transmission (a VDF, dropped) and dispersion.
const NOT_CARRIED: [&str; 5] = [
    "transmission_depth",
    "transmission_scatter",
    "transmission_scatter_anisotropy",
    "transmission_dispersion_scale",
    "transmission_dispersion_abbe_number",
];

#[test]
fn open_pbr_surface_distills_back_to_its_fields() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/materialx/resources/Materials/Examples/OpenPbr");
    // The examples without a coat (whose darkening of the base is not undone) or a medium (dropped).
    let examples = ["default", "aluminum_brushed", "glass", "velvet", "lightbulb", "soapbubble"];
    for example in examples {
        let file = root.join(format!("open_pbr_{example}.mtlx"));
        let location = file.display().to_string();
        let text = std::fs::read_to_string(&file).unwrap();
        let model = IrOptions::with_models(&["open_pbr_surface"]);
        let original = shadewire_mtlx::to_ir(&text, Some(&location), None, &model, &Default::default()).unwrap().dag;
        let lobes = IrOptions::with_models(&LOBE_MODELS);
        let mut dag = shadewire_mtlx::to_ir(&text, Some(&location), None, &lobes, &Default::default()).unwrap().dag;
        distill(&mut dag).unwrap();
        for (want, got) in evaluate(&original).into_iter().zip(evaluate(&dag)) {
            for ((name, a), (other, b)) in want.into_iter().zip(got) {
                assert_eq!(name, other);
                if NOT_CARRIED.contains(&name.as_str()) {
                    continue;
                }
                let close = a.floats().iter().zip(b.floats()).all(|(x, y)| (x - y).abs() <= 1e-3 * (1.0 + x.abs()));
                assert!(close, "{example}: {name} {a:?} distilled to {b:?}");
            }
        }
    }
}

#[test]
fn an_mdl_fresnel_layer_over_a_diffuse_distills_exactly() {
    let path = fixtures_dir().join("omni_like.mdl");
    let location = path.to_string_lossy().to_string();
    let text = std::fs::read_to_string(&path).unwrap();
    let options = IrOptions { values_as_uniforms: false, ..distilling() };
    let ir = to_ir(&text, Some(&location), None, &options, &Resolver::default()).unwrap();
    let values = &evaluate(&ir.dag)[0];
    let get = |name: &str| values.iter().find(|(n, _)| n == name).unwrap().1.floats();
    let near = |name: &str, want: &[f32]| {
        let got = get(name);
        assert!(got.iter().zip(want).all(|(g, w)| (g - w).abs() < 1e-4), "{name}: {got:?}, expected {want:?}");
    };
    near("base_color", &[0.2, 0.2, 0.2]);
    near("base_weight", &[1.0]);
    near("base_metalness", &[0.0]);
    near("specular_weight", &[1.0]);
    near("specular_ior", &[1.5]);
    // MDL's roughness is the microfacet alpha: open_pbr_surface's roughness is its square root.
    near("specular_roughness", &[0.5f32.sqrt()]);
    near("emission_luminance", &[0.0]);
    near("geometry_opacity", &[1.0]);
    assert!(ir.dag.approximations.is_empty(), "{:?}", ir.dag.approximations);
}

#[test]
fn without_distill_an_mdl_material_still_needs_the_surface_model() {
    let path = fixtures_dir().join("diffuse.mdl");
    let location = path.to_string_lossy().to_string();
    let text = std::fs::read_to_string(&path).unwrap();
    let options = IrOptions::with_models(&["open_pbr_surface"]);
    assert!(to_ir(&text, Some(&location), None, &options, &Resolver::default()).is_err());
}
