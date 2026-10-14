//! The assembled module compiles the way Bevy compiles it: `wesl` 0.6 (bevy_shader 0.20's compiler and
//! options, bevy_shader src/shader_cache.rs:223-290) resolving Bevy's own WESL sources from the cargo
//! registry at the versions in Cargo.lock, then naga validation of the WGSL it produces.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use shadewire_bevy::compile_options;
use shadewire_mtlx::{MtlxOptions, to_ir};
use shadewire_wgsl::compile;
use wesl::error::ResolveError;
use wesl::syntax::{ModulePath, PathOrigin};

const WORKSPACE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const BEVY_PACKAGES: [&str; 3] = ["bevy_pbr", "bevy_render", "bevy_core_pipeline"];

/// A forward main pass over a mesh with every attribute, tonemapped in shader, clearcoat and
/// transmission on (bevy_pbr src/render/mesh.rs:3399-3721, pbr_material.rs:1495-1592).
const FULL: &[&str] = &[
    "MESH_PIPELINE",
    "VERTEX_OUTPUT_INSTANCE_INDEX",
    "VERTEX_POSITIONS",
    "VERTEX_NORMALS",
    "VERTEX_UVS",
    "VERTEX_UVS_A",
    "VERTEX_UVS_B",
    "VERTEX_TANGENTS",
    "VERTEX_COLORS",
    "TONEMAP_IN_SHADER",
    "TONEMAP_METHOD_TONY_MC_MAPFACE",
    "DEBAND_DITHER",
    "VIEW_PROJECTION_PERSPECTIVE",
    "SHADOW_FILTER_METHOD_GAUSSIAN",
    "PCSS_SAMPLERS_AVAILABLE",
    "MAY_DISCARD",
    "VISIBILITY_RANGE_DITHER",
    "STANDARD_MATERIAL_CLEARCOAT",
    "STANDARD_MATERIAL_SPECULAR_TRANSMISSION",
    "VIEW_TRANSMISSION_TEXTURE",
];

/// [`FULL`] with contact shadows, which the lobe path fetches itself.
const CONTACT: &[&str] = &[
    "MESH_PIPELINE",
    "VERTEX_OUTPUT_INSTANCE_INDEX",
    "VERTEX_POSITIONS",
    "VERTEX_NORMALS",
    "VERTEX_UVS",
    "VERTEX_UVS_A",
    "VERTEX_TANGENTS",
    "TONEMAP_IN_SHADER",
    "TONEMAP_METHOD_TONY_MC_MAPFACE",
    "VIEW_PROJECTION_PERSPECTIVE",
    "SHADOW_FILTER_METHOD_GAUSSIAN",
    "MAY_DISCARD",
    "CONTACT_SHADOWS",
    "DEPTH_PREPASS",
];

/// Positions and normals only: the screen-space tangent, zero UVs and white color paths.
const BARE: &[&str] = &["MESH_PIPELINE", "VERTEX_OUTPUT_INSTANCE_INDEX", "VERTEX_POSITIONS", "VERTEX_NORMALS"];

/// Bevy's numeric defs (bevy_pbr src/render/light.rs:236-246, mesh_view_bindings.rs:71-72, mesh.rs:3705, material.rs:521).
const CONSTANTS: &[(&str, u32)] = &[
    ("MATERIAL_BIND_GROUP", 3),
    ("MAX_DIRECTIONAL_LIGHTS", 10),
    ("MAX_CASCADES_PER_LIGHT", 4),
    ("MAX_RECT_LIGHTS", 8),
    ("TONEMAPPING_LUT_TEXTURE_BINDING_INDEX", 18),
    ("TONEMAPPING_LUT_SAMPLER_BINDING_INDEX", 19),
    ("SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS", 8),
];

/// Serves `shadewire` (the module under test), the virtual `constants` module Bevy generates from
/// its numeric defs, and `<package>::a::b` from `<registry>/<package>-<version>/src/a/b.wesl`.
struct BevySources {
    roots: HashMap<String, PathBuf>,
    main: String,
    constants: String,
}

impl wesl::Resolver for BevySources {
    fn resolve_source<'a>(&'a self, path: &ModulePath) -> Result<Cow<'a, str>, ResolveError> {
        let path = &self.canonical_path(path);
        let missing = |why: &str| ResolveError::ModuleNotFound(path.clone(), why.to_string());
        match &path.origin {
            PathOrigin::Absolute if path.components == ["shadewire"] => Ok(Cow::Borrowed(&self.main)),
            PathOrigin::Package(p) if p == "constants" && path.components.is_empty() => {
                Ok(Cow::Borrowed(&self.constants))
            }
            PathOrigin::Package(p) => {
                let root = self.roots.get(p).ok_or_else(|| missing("not a Bevy package under test"))?;
                let mut file = root.join("src");
                for c in &path.components {
                    file.push(c);
                }
                file.set_extension("wesl");
                std::fs::read_to_string(&file).map(Cow::Owned).map_err(|e| missing(&format!("{}: {e}", file.display())))
            }
            _ => Err(missing("unexpected origin")),
        }
    }

    /// A package import seen from inside a package arrives as `importer/package`; Bevy keeps the last
    /// part (bevy_shader src/shader_cache.rs:509-521).
    fn canonical_path(&self, path: &ModulePath) -> ModulePath {
        match &path.origin {
            PathOrigin::Package(p) if p.contains('/') => ModulePath {
                origin: PathOrigin::Package(p.rsplit('/').next().unwrap().to_string()),
                components: path.components.clone(),
            },
            _ => path.clone(),
        }
    }
}

/// The version of `package` in the workspace lockfile.
fn locked_version(lock: &str, package: &str) -> String {
    let marker = format!("name = \"{package}\"\nversion = \"");
    let start = lock.find(&marker).unwrap_or_else(|| panic!("{package} is not in Cargo.lock")) + marker.len();
    lock[start..].split('"').next().unwrap().to_string()
}

fn cargo_home() -> PathBuf {
    if let Some(home) = std::env::var_os("CARGO_HOME") {
        return PathBuf::from(home);
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).expect("no home directory");
    Path::new(&home).join(".cargo")
}

/// `<cargo home>/registry/src/<index>/<package>-<version>` for each Bevy package.
fn bevy_roots() -> HashMap<String, PathBuf> {
    let lock = std::fs::read_to_string(Path::new(WORKSPACE).join("Cargo.lock")).unwrap().replace("\r\n", "\n");
    let registry = cargo_home().join("registry").join("src");
    let indexes: Vec<PathBuf> = std::fs::read_dir(&registry)
        .unwrap_or_else(|e| panic!("{}: {e}", registry.display()))
        .map(|entry| entry.unwrap().path())
        .collect();
    let mut roots = HashMap::new();
    for package in BEVY_PACKAGES {
        let dir = format!("{package}-{}", locked_version(&lock, package));
        let root = indexes.iter().map(|index| index.join(&dir)).find(|p| p.is_dir());
        let root = root.unwrap_or_else(|| panic!("{dir} sources not found under {}", registry.display()));
        roots.insert(package.to_string(), root);
    }
    roots
}

/// The module for `file` (relative to the workspace), compiled by wesl under `defs` then validated by naga.
fn compile_in_bevy(file: &str, defs: &[&str]) -> (String, naga::Module) {
    compile_models_in_bevy(file, defs, &MtlxOptions::default().models)
}

/// [`compile_in_bevy`] with the front end accepting `models`.
fn compile_models_in_bevy(file: &str, defs: &[&str], models: &[String]) -> (String, naga::Module) {
    let path = Path::new(WORKSPACE).join(file);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let options = MtlxOptions { models: models.to_vec(), ..MtlxOptions::default() };
    let ir = to_ir(&text, path.to_str(), None, &options).unwrap();
    let compiled = compile(&ir.dag, &ir.source, &compile_options()).unwrap();
    let main = shadewire_bevy::shader::module(&compiled).unwrap();
    let wgsl = wesl_to_wgsl(&main, defs).unwrap_or_else(|e| panic!("{file}: wesl:\n{e}\n\nmodule:\n{main}"));
    let module = naga::front::wgsl::parse_str(&wgsl).unwrap_or_else(|e| panic!("{file}: naga parse: {e}"));
    let mut validator =
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all());
    validator.validate(&module).unwrap_or_else(|e| panic!("{file}: naga validation: {e:?}"));
    (main, module)
}

fn wesl_to_wgsl(main: &str, defs: &[&str]) -> Result<String, String> {
    let mut options = wesl::CompileOptions { imports: true, condcomp: true, visibility: false, ..Default::default() };
    for def in defs {
        options.features.flags.insert(def.to_string(), true.into());
    }
    let mut constants = String::new();
    for (name, value) in CONSTANTS {
        options.features.flags.insert(name.to_string(), true.into());
        constants.push_str(&format!("const {name} = {value};\n"));
    }
    let resolver = BevySources { roots: bevy_roots(), main: main.to_string(), constants };
    let entry = ModulePath { origin: PathOrigin::Absolute, components: vec!["shadewire".to_string()] };
    let result = wesl::compile(&entry, &options, &resolver).map_err(|e| e.to_string())?;
    Ok(result.to_string())
}

/// The (group, binding) of every global the module binds in the material group.
fn material_bindings(module: &naga::Module) -> Vec<u32> {
    let mut out: Vec<u32> = module
        .global_variables
        .iter()
        .filter_map(|(_, g)| g.binding.as_ref())
        .filter(|b| b.group == 3 && b.binding >= 100)
        .map(|b| b.binding)
        .collect();
    out.sort();
    out
}

const EXAMPLES: &str = "vendor/materialx/resources/Materials/Examples";

#[test]
fn standard_surface_default_compiles() {
    let file = format!("{EXAMPLES}/StandardSurface/standard_surface_default.mtlx");
    for defs in [FULL, BARE] {
        let (main, module) = compile_in_bevy(&file, defs);
        assert!(main.contains("pbr_input.material.ior = s.specular_IOR;"));
        assert!(module.entry_points.iter().any(|e| e.name == "fragment"));
        assert_eq!(material_bindings(&module), [100]);
    }
}

#[test]
fn standard_surface_marble_compiles() {
    let (main, _) = compile_in_bevy(&format!("{EXAMPLES}/StandardSurface/standard_surface_marble_solid.mtlx"), FULL);
    assert!(main.contains("shadewire_in_position_object = (shadewire_local_from_world"));
}

#[test]
fn textured_standard_surface_binds_101_and_102() {
    let (_, module) = compile_in_bevy(&format!("{EXAMPLES}/StandardSurface/standard_surface_brass_tiled.mtlx"), FULL);
    let bindings = material_bindings(&module);
    assert!(bindings.starts_with(&[100, 101, 102]), "{bindings:?}");
}

#[test]
fn open_pbr_default_compiles() {
    let (main, _) = compile_in_bevy(&format!("{EXAMPLES}/OpenPbr/open_pbr_default.mtlx"), FULL);
    assert!(main.contains("s.emission_luminance / 1000.0 * s.emission_color"));
}

#[test]
fn gltf_pbr_default_compiles() {
    let (main, _) = compile_in_bevy(&format!("{EXAMPLES}/GltfPbr/gltf_pbr_default.mtlx"), BARE);
    assert!(main.contains("pbr_input.material.emissive = vec4<f32>(s.emissive * s.emissive_strength"));
}

#[test]
fn surface_unlit_compiles() {
    let (main, _) = compile_in_bevy("tools/oracle/materials/unlit_basic.mtlx", FULL);
    assert!(main.contains("shadewire_in_texcoord_0 = vec2<f32>(shadewire_uv0.x, 1.0 - shadewire_uv0.y);"));
    assert!(main.contains("pbr_input.material.flags |= STANDARD_MATERIAL_FLAGS_UNLIT_BIT;"));
}

#[test]
fn every_host_read_compiles() {
    let (main, _) = compile_in_bevy("crates/shadewire-bevy/tests/host_reads.mtlx", FULL);
    for read in ["position_object", "normal_object", "tangent_model", "bitangent_world", "texcoord_1", "geomcolor_0"] {
        assert!(main.contains(&format!("shadewire_in_{read} = ")), "{read}");
    }
    for read in ["view_direction_world", "time", "frame"] {
        assert!(main.contains(&format!("shadewire_in_{read} = ")), "{read}");
    }
    assert!(main.contains("fn shadewire_host_transform_point_object_to_world("));
    assert!(main.contains("var<private> shadewire_in_geomprop_tint: vec3<f32> = vec3<f32>(0.5, 0.5, 0.5);"));
    compile_in_bevy("crates/shadewire-bevy/tests/host_reads.mtlx", BARE);
}

fn lobe_models() -> Vec<String> {
    shadewire::LOBE_MODELS.iter().map(|m| m.to_string()).collect()
}

#[test]
fn closure_trees_are_lit_by_their_lobes() {
    for file in [
        "StandardSurface/standard_surface_default.mtlx",
        "OpenPbr/open_pbr_default.mtlx",
        "GltfPbr/gltf_pbr_default.mtlx",
        "StandardSurface/standard_surface_brass_tiled.mtlx",
    ] {
        for defs in [FULL, BARE, CONTACT] {
            let (main, module) = compile_models_in_bevy(&format!("{EXAMPLES}/{file}"), defs, &lobe_models());
            assert!(
                main.contains(
                    "    let s: Surface = shadewire_eval_surface();
"
                ),
                "{file}"
            );
            assert!(main.contains("out.color = shadewire_light(pbr_input, s, shadewire_n);"), "{file}");
            assert!(!main.contains("apply_pbr_lighting(pbr_input)"), "{file}");
            assert!(module.entry_points.iter().any(|e| e.name == "fragment"), "{file}");
        }
    }
}

#[test]
fn a_closure_trees_opacity_is_the_alpha() {
    let file = format!("{EXAMPLES}/GltfPbr/gltf_pbr_default.mtlx");
    let (main, _) = compile_models_in_bevy(&file, BARE, &lobe_models());
    assert!(main.contains("pbr_input.material.base_color = vec4<f32>(pbr_input.material.base_color.rgb, s."), "{main}");
    assert!(!main.contains("base_color.rgb, 1.0);"), "{main}");
}
