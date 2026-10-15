//! Deterministic probes and the "same material" assertions a front end's round-trip tests use (feature test-support).

use crate::interpret::{ContextSource, TextureSource, eval_surface};
use crate::{ContextRead, Dag, NodeKind, Space, TextureDesc, TransformKind, Type, Value};
use std::collections::BTreeSet;

/// A deterministic host: probe `k` (0..4) gives every read a fixed value; transforms are the identity.
pub struct Probe(pub u32);

/// The four probes the round trip evaluates both materials at.
pub fn probes() -> Vec<Probe> {
    (0..4).map(Probe).collect()
}

/// The first `ty.component_count()` of `floats`, as a value of `ty`.
fn cut(ty: Type, floats: &[f32]) -> Value {
    let mut floats = floats.to_vec();
    floats.resize(ty.component_count(), 0.0);
    Value::from_floats(ty, &floats)
}

impl ContextSource for Probe {
    fn read(&mut self, read: &ContextRead) -> Value {
        let k = self.0 as f32;
        match read {
            ContextRead::Position(_) => Value::Vector3([0.1 + 0.2 * k, 0.3, 0.7 - 0.1 * k]),
            ContextRead::Normal(_) => {
                let n = [0.1 * k, 1.0, 0.2];
                let length = n.iter().map(|c| c * c).sum::<f32>().sqrt();
                Value::Vector3(n.map(|c| c / length))
            }
            ContextRead::Tangent(_) => Value::Vector3([1.0, 0.0, 0.0]),
            ContextRead::Bitangent(_) => Value::Vector3([0.0, 0.0, 1.0]),
            ContextRead::TexCoord { ty, .. } => cut(*ty, &[0.15 + 0.2 * k, 0.8 - 0.15 * k]),
            ContextRead::GeomColor { ty, .. } => cut(*ty, &[0.2, 0.4, 0.6, 1.0]),
            ContextRead::GeomProp { ty, .. } => {
                let zero = Value::zero(*ty).floats();
                Value::from_floats(*ty, &zero.iter().map(|c| c + 0.25).collect::<Vec<f32>>())
            }
            ContextRead::ViewDirection(_) => Value::Vector3([0.0, 0.0, 1.0]),
            ContextRead::Time => Value::Float(1.5),
            ContextRead::Frame => Value::Float(12.0),
        }
    }

    fn transform(&mut self, _: TransformKind, _: Space, _: Space, v: [f32; 3]) -> [f32; 3] {
        v
    }
}

/// A deterministic texture: every channel varies with the uv and the file, inside `0.25..0.75`, so a colorspace
/// conversion, a uv transform or a different file changes what a sample reads.
pub struct ProbeTextures;

impl TextureSource for ProbeTextures {
    fn sample(&self, texture: &TextureDesc, uv: [f32; 2]) -> Option<[f32; 4]> {
        // FNV-1a of the file, folded into 0..1.
        let hash = texture.file.bytes().fold(0x811c_9dc5_u32, |h, b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193));
        let h = (hash % 1000) as f32 / 1000.0;
        let f = |c: f32| 0.25 + 0.5 * (uv[0] * 3.7 + uv[1] * 5.3 + c * 0.31 + h).rem_euclid(1.0);
        Some([f(0.0), f(1.0), f(2.0), 1.0])
    }
}

fn uniforms(dag: &Dag) -> BTreeSet<(String, String)> {
    let named = dag.nodes.iter().filter_map(|n| match &n.kind {
        NodeKind::Uniform { name, value } => Some((name.clone(), format!("{value:?}"))),
        _ => None,
    });
    named.collect()
}

/// The writer's definition of correct: `b` (read from what `a` wrote) is the same material as `a`.
pub fn assert_same_material(a: &Dag, b: &Dag, written: &str) {
    same_material(a, b, written, true);
}

/// `assert_same_material`, the textures compared without their colorspace: a hextiled image converts its blend, not
/// its samples, so the conversion is written as nodes of its own and the samples' colorspace nowhere.
pub fn assert_same_material_but_texture_colorspaces(a: &Dag, b: &Dag, written: &str) {
    same_material(a, b, written, false);
}

fn same_material(a: &Dag, b: &Dag, written: &str, texture_colorspaces: bool) {
    assert_eq!(a.surface.model, b.surface.model, "model\n{written}");
    // `Type` is not `Ord`: fields are compared by name and type name.
    let fields = |d: &Dag| d.surface.fields.iter().map(|f| (f.name.clone(), f.ty.mtlx_name())).collect::<BTreeSet<_>>();
    assert_eq!(fields(a), fields(b), "fields\n{written}");
    assert_eq!(uniforms(a), uniforms(b), "uniforms\n{written}");
    assert_eq!(a.uniform_ui, b.uniform_ui, "uniform ui hints\n{written}");
    assert_eq!(a.geomprop_defaults, b.geomprop_defaults, "geomprop defaults\n{written}");
    let texture = |t: &TextureDesc| match texture_colorspaces {
        true => format!("{t:?}"),
        false => format!("{:?}", TextureDesc { colorspace: None, ..t.clone() }),
    };
    let textures = |d: &Dag| d.textures.iter().map(texture).collect::<BTreeSet<_>>();
    assert_eq!(textures(a), textures(b), "textures\n{written}");
    assert_eq!(a.surface.closure, b.surface.closure, "closure tree\n{written}");
    assert_same_evaluation(a, b, written);
}

/// Every field of `a`, by name, evaluates in `b` to the same value at the four probes, textures read through
/// `ProbeTextures`.
pub fn assert_same_evaluation(a: &Dag, b: &Dag, written: &str) {
    for mut probe in probes() {
        let k = probe.0;
        let in_a = eval_surface(a, &mut probe, &ProbeTextures);
        let in_b = eval_surface(b, &mut probe, &ProbeTextures);
        for (name, value) in &in_a {
            let (_, other) = in_b.iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("{name}\n{written}"));
            let same = |(x, y): (&f32, f32)| (x - y).abs() <= 1e-5 || (x.is_nan() && y.is_nan());
            let close = value.floats().iter().zip(other.floats()).all(same);
            assert!(close, "probe {k}, field {name}: {value:?} became {other:?}\n{written}");
        }
    }
}
