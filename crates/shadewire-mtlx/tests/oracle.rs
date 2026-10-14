//! Upstream oracle: MaterialX's own GLSL, baked on a 16x16 uv quad by
//! `tools/oracle/bake_oracle.py`, against shadewire's interpreter.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};

use shadewire::Dag;
use shadewire::interpret::{ContextSource, TextureSource, eval_surface};
use shadewire::optimize::optimize;
use shadewire::{AddressMode, ContextRead, FilterMode, NodeKind, Space, TextureDesc, TransformKind, Value};
mod common;

use common::compile_mtlx;
use shadewire_nodes::LowerOptions;
use shadewire_wgsl::CompileOptions;

const TOLERANCE: f32 = 2e-3;
const IMAGE_TOLERANCE: f32 = 1e-2;
const UV_PROBE_TOLERANCE: f32 = 1e-3;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// One baked material: per surface input, `width * height` RGBA texels, row-major as captured.
struct Baked {
    material: String,
    width: usize,
    height: usize,
    fields: Vec<(String, Vec<[f32; 4]>)>,
}

fn load_baked(path: &Path) -> Baked {
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut fields = Vec::new();
    for (name, texels) in json["fields"].as_object().unwrap() {
        let texels = texels
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                let c: Vec<f32> = t.as_array().unwrap().iter().map(|x| x.as_f64().unwrap() as f32).collect();
                [c[0], c[1], c[2], c[3]]
            })
            .collect();
        fields.push((name.clone(), texels));
    }
    Baked {
        material: json["material"].as_str().unwrap().to_string(),
        width: json["width"].as_u64().unwrap() as usize,
        height: json["height"].as_u64().unwrap() as usize,
        fields,
    }
}

/// The oracle materials read only texture coordinates; anything else is a broken material.
struct TexCoordOnly([f32; 2]);

impl ContextSource for TexCoordOnly {
    fn read(&mut self, read: &ContextRead) -> Value {
        match read {
            ContextRead::TexCoord { index: 0, .. } => Value::Vector2(self.0),
            other => panic!("oracle materials read only texcoord 0, not {other:?}"),
        }
    }
    fn transform(&mut self, kind: TransformKind, from: Space, to: Space, _v: [f32; 3]) -> [f32; 3] {
        panic!("oracle materials transform nothing ({kind:?} {from:?} -> {to:?})")
    }
}

/// An 8-bit image, top row first, as the `png` crate decodes it.
struct Image {
    width: usize,
    height: usize,
    rgba: Vec<[f32; 4]>,
}

fn load_png(path: &Path) -> Image {
    let mut decoder = png::Decoder::new(File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display())));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    let channels = info.color_type.samples();
    let rgba = buf[..info.buffer_size()]
        .chunks(channels)
        .map(|p| {
            let f = |i: usize| p[i] as f32 / 255.0;
            match channels {
                1 => [f(0), f(0), f(0), 1.0],
                2 => [f(0), f(0), f(0), f(1)],
                3 => [f(0), f(1), f(2), 1.0],
                _ => [f(0), f(1), f(2), f(3)],
            }
        })
        .collect();
    Image { width: info.width as usize, height: info.height as usize, rgba }
}

/// Closest or linear (GL's, texel centers at +0.5) filter, periodic wrap; images are
/// stored top row first. Oracle uvs stay magnified, so mip level 0 is what GL reads.
struct PngTextures(HashMap<String, Image>);

impl PngTextures {
    fn for_dag(dag: &Dag) -> PngTextures {
        let mut images = HashMap::new();
        for texture in &dag.textures {
            assert_ne!(texture.filter, FilterMode::Cubic, "oracle images use the closest or linear filter");
            assert_eq!((texture.address_u, texture.address_v), (AddressMode::Periodic, AddressMode::Periodic));
            let path = root().join("vendor/materialx").join(&texture.file);
            images.insert(texture.file.clone(), load_png(&path));
        }
        PngTextures(images)
    }
}

impl TextureSource for PngTextures {
    fn sample(&self, texture: &TextureDesc, uv: [f32; 2]) -> Option<[f32; 4]> {
        let image = &self.0[&texture.file];
        let texel = |x: i64, y: i64| {
            let x = x.rem_euclid(image.width as i64) as usize;
            let y = y.rem_euclid(image.height as i64) as usize;
            image.rgba[y * image.width + x]
        };
        let x = uv[0] * image.width as f32;
        let y = (1.0 - uv[1]) * image.height as f32;
        if texture.filter == FilterMode::Closest {
            return Some(texel(x.floor() as i64, y.floor() as i64));
        }
        let (x, y) = (x - 0.5, y - 0.5);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let (a, b, c, d) = (texel(x0, y0), texel(x0 + 1, y0), texel(x0, y0 + 1), texel(x0 + 1, y0 + 1));
        let mut out = [0.0; 4];
        for i in 0..4 {
            let top = a[i] + (b[i] - a[i]) * fx;
            let bottom = c[i] + (d[i] - c[i]) * fx;
            out[i] = top + (bottom - top) * fy;
        }
        Some(out)
    }
}

/// The material's DAG cut down to the baked fields (the others read normals and tangents).
fn compile(material: &str, fields: &[String]) -> Dag {
    let path = root().join("tools/oracle/materials").join(format!("{material}.mtlx"));
    let text = std::fs::read_to_string(&path).unwrap();
    let lower = LowerOptions { values_as_uniforms: false, ..LowerOptions::default() };
    let mut dag = compile_mtlx(&text, Some(&path), None, &lower, &CompileOptions::default())
        .unwrap_or_else(|e| panic!("{material}: {e}"))
        .dag;
    dag.surface.fields.retain(|f| fields.contains(&f.name));
    assert_eq!(dag.surface.fields.len(), fields.len(), "{material}: every baked field is a surface field");
    let dag = optimize(&dag);
    for node in &dag.nodes {
        if let NodeKind::Context(read) = &node.kind {
            assert!(matches!(read, ContextRead::TexCoord { index: 0, .. }), "{material} reads {read:?}");
        }
    }
    dag
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum VConvention {
    Up,
    Down,
}

fn uv_at(x: usize, y: usize, w: usize, h: usize, convention: VConvention) -> [f32; 2] {
    let u = (x as f32 + 0.5) / w as f32;
    let v = (y as f32 + 0.5) / h as f32;
    match convention {
        VConvention::Up => [u, v],
        VConvention::Down => [u, 1.0 - v],
    }
}

/// The largest per-channel error of each baked field, with the texel and both values.
struct FieldError {
    field: String,
    max: f32,
    at: (usize, usize),
    ours: Vec<f32>,
    theirs: [f32; 4],
}

fn compare(baked: &Baked, convention: VConvention) -> Vec<FieldError> {
    let names: Vec<String> = baked.fields.iter().map(|(n, _)| n.clone()).collect();
    let dag = compile(&baked.material, &names);
    let textures = PngTextures::for_dag(&dag);
    let mut errors: Vec<FieldError> = baked
        .fields
        .iter()
        .map(|(name, _)| FieldError { field: name.clone(), max: -1.0, at: (0, 0), ours: Vec::new(), theirs: [0.0; 4] })
        .collect();
    for y in 0..baked.height {
        for x in 0..baked.width {
            let uv = uv_at(x, y, baked.width, baked.height, convention);
            let surface = eval_surface(&dag, &mut TexCoordOnly(uv), &textures);
            for (error, (name, texels)) in errors.iter_mut().zip(&baked.fields) {
                let ours =
                    surface.iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("no field {name}")).1.floats();
                let theirs = texels[y * baked.width + x];
                for (c, ours_c) in ours.iter().enumerate() {
                    let e = (ours_c - theirs[c]).abs();
                    // A NaN error sticks: no later texel may hide it.
                    if !error.max.is_nan() && (e.is_nan() || e > error.max) {
                        *error = FieldError { field: name.clone(), max: e, at: (x, y), ours: ours.clone(), theirs };
                    }
                }
            }
        }
    }
    errors
}

fn worst(errors: &[FieldError]) -> f32 {
    // NaN-propagating: f32::max would drop a NaN error.
    errors.iter().map(|e| e.max).fold(0.0, |a, b| if a.is_nan() || b.is_nan() { f32::NAN } else { a.max(b) })
}

fn describe(material: &str, errors: &[FieldError]) -> String {
    errors
        .iter()
        .map(|e| {
            format!("{material}.{}: max {:.3e} at {:?} ours {:?} theirs {:?}", e.field, e.max, e.at, e.ours, e.theirs)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn oracle_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle")
}

/// `uv_probe` decides how texel (x, y) maps to uv: exactly one convention must match.
fn uv_convention() -> VConvention {
    let probe = load_baked(&oracle_dir().join("uv_probe.json"));
    let up = compare(&probe, VConvention::Up);
    let down = compare(&probe, VConvention::Down);
    let matches: Vec<VConvention> = [(VConvention::Up, &up), (VConvention::Down, &down)]
        .iter()
        .filter(|(_, e)| worst(e) <= UV_PROBE_TOLERANCE)
        .map(|(c, _)| *c)
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "uv convention undecided:\nv up:\n{}\nv down:\n{}",
        describe("uv_probe", &up),
        describe("uv_probe", &down)
    );
    matches[0]
}

/// Materials that sample an image.
const IMAGE_MATERIALS: [&str; 2] = ["hextile_grid", "image_grid"];

/// Materials whose graph also drives `specular_roughness` (the scalar ops).
const SCALAR_MATERIALS: [&str; 8] =
    ["cellnoise2d", "fractal3d", "logic_switch", "math_ops", "noise2d", "noise3d", "trig_ops", "worley2d"];

fn expected_fields(material: &str) -> Vec<&'static str> {
    if material == "unlit_basic" {
        vec!["emission"]
    } else if SCALAR_MATERIALS.contains(&material) {
        vec!["base_color", "specular_roughness"]
    } else {
        vec!["base_color"]
    }
}

fn field_names(baked: &Baked) -> Vec<&str> {
    let mut names: Vec<&str> = baked.fields.iter().map(|(n, _)| n.as_str()).collect();
    names.sort();
    names
}

#[test]
fn interpreter_matches_materialx_glsl() {
    let convention = uv_convention();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(oracle_dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 17, "one baked json per oracle material");
    let mut report = Vec::new();
    let mut failed = false;
    for path in &paths {
        let baked = load_baked(path);
        // GL filters with 8-bit weights: linear (hextile_grid) moves texels by up to 1/256.
        let tolerance = if IMAGE_MATERIALS.contains(&baked.material.as_str()) { IMAGE_TOLERANCE } else { TOLERANCE };
        let errors = compare(&baked, convention);
        assert_eq!(field_names(&baked), expected_fields(&baked.material), "{} baked fields", baked.material);
        let w = worst(&errors);
        failed |= w.is_nan() || w > tolerance;
        report.push(describe(&baked.material, &errors));
    }
    println!("uv convention {convention:?}\n{}", report.join("\n"));
    assert!(!failed, "oracle mismatch (tolerance {TOLERANCE:e}, images {IMAGE_TOLERANCE:e}):\n{}", report.join("\n"));
}
