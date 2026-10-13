//! The support libraries: ports of MaterialX's genglsl helpers (color, normal maps, noise, hextiling) and its
//! pbrlib lobes, written once in the WGSL subset [`super::parse`] reads, parsed and typed into the tree every
//! back end prints. A material's code includes only the files it calls into.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::ast::{Item, Module};
use super::infer::{Env, infer};
use super::parse::parse;
use crate::reflection::live::live_nodes;
use crate::{ClosureNode, Dag, NodeKind, Op};

/// Every library file, in dependency order.
pub const FILES: [&str; 14] = [
    "color",
    "normal",
    "vector",
    "hextile",
    "noise_basics",
    "noise_hash",
    "noise_perlin",
    "noise_fractal",
    "noise_cell",
    "noise_worley",
    "closure_base",
    "closure_diffuse",
    "closure_specular",
    "closure_sheen",
];

/// The source of one library file. Panics on an unknown name.
pub fn source(name: &str) -> &'static str {
    match name {
        "color" => include_str!("lib/color.wgsl"),
        "normal" => include_str!("lib/normal.wgsl"),
        "vector" => include_str!("lib/vector.wgsl"),
        "hextile" => include_str!("lib/hextile.wgsl"),
        "noise_basics" => include_str!("lib/noise_basics.wgsl"),
        "noise_hash" => include_str!("lib/noise_hash.wgsl"),
        "noise_perlin" => include_str!("lib/noise_perlin.wgsl"),
        "noise_fractal" => include_str!("lib/noise_fractal.wgsl"),
        "noise_cell" => include_str!("lib/noise_cell.wgsl"),
        "noise_worley" => include_str!("lib/noise_worley.wgsl"),
        "closure_base" => include_str!("lib/closure_base.wgsl"),
        "closure_diffuse" => include_str!("lib/closure_diffuse.wgsl"),
        "closure_specular" => include_str!("lib/closure_specular.wgsl"),
        "closure_sheen" => include_str!("lib/closure_sheen.wgsl"),
        other => panic!("no library file {other:?}"),
    }
}

/// The library files `op` calls into, with their dependencies.
pub fn needs(op: &Op) -> &'static [&'static str] {
    match op {
        Op::RgbToHsv | Op::HsvToRgb => &["color"],
        Op::NormalMap => &["normal"],
        Op::Rotate2d => &["vector"],
        Op::HextileCoord(_) | Op::HextileGrad(_) | Op::HextileWeights => &["hextile"],
        Op::Noise2d | Op::Noise3d => &["noise_basics", "noise_hash", "noise_perlin"],
        Op::Fractal3d => &["noise_basics", "noise_hash", "noise_perlin", "noise_fractal"],
        Op::CellNoise2d | Op::CellNoise3d => &["noise_basics", "noise_hash", "noise_cell"],
        Op::WorleyNoise2d | Op::WorleyNoise3d => &["noise_basics", "noise_hash", "noise_cell", "noise_worley"],
        _ => &[],
    }
}

/// The library files a material needs, each once, in dependency order: those its live ops call, and with a
/// closure tree the closure files of its lobes.
pub fn names_for(dag: &Dag) -> Vec<&'static str> {
    let live = live_nodes(dag);
    let mut wanted: Vec<&'static str> = Vec::new();
    for (i, node) in dag.nodes.iter().enumerate() {
        if let (true, NodeKind::Op { op, .. }) = (live[i], &node.kind) {
            wanted.extend_from_slice(needs(op));
        }
    }
    if let Some(graph) = &dag.surface.closure {
        wanted.push("closure_base");
        for node in &graph.nodes {
            if let ClosureNode::Lobe { kind, .. } = node
                && let Some(file) = super::material::closure::lobe_file(kind)
            {
                wanted.push(file);
            }
        }
    }
    FILES.into_iter().filter(|f| wanted.contains(f)).collect()
}

/// Every library file parsed and typed, in dependency order; each file sees the declarations of the files
/// before it. Done once per process.
fn parsed() -> &'static HashMap<&'static str, Module> {
    static PARSED: OnceLock<HashMap<&'static str, Module>> = OnceLock::new();
    PARSED.get_or_init(|| {
        let mut env = Env::default();
        let mut out = HashMap::new();
        for name in FILES {
            let mut module = parse(source(name)).unwrap_or_else(|e| panic!("library {name}: {e}"));
            infer(&mut module, &mut env).unwrap_or_else(|e| panic!("library {name}: {e}"));
            out.insert(name, module);
        }
        out
    })
}

/// One library file as a typed tree. Panics on an unknown name.
pub fn module(name: &str) -> &'static Module {
    parsed().get(name).unwrap_or_else(|| panic!("no library file {name:?}"))
}

/// The items of the library files a material needs, in dependency order.
pub fn items_for(dag: &Dag) -> Vec<Item> {
    names_for(dag).into_iter().flat_map(|name| module(name).items.iter().cloned()).collect()
}

/// The declarations of every library file: what the material's code can call.
pub fn env() -> Env {
    let mut env = Env::default();
    for name in FILES {
        env.declare(module(name));
    }
    env
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SurfaceField, Type, Value};

    #[test]
    fn every_file_parses_and_types() {
        for name in FILES {
            assert!(!module(name).items.is_empty(), "{name}");
        }
    }

    #[test]
    fn every_file_carries_the_attribution() {
        for name in FILES {
            let text = source(name);
            if name.starts_with("closure_") {
                assert!(text.contains("ported from MaterialX pbrlib genglsl"), "{name}");
                continue;
            }
            assert!(text.starts_with("// Port of MaterialX libraries/stdlib/genglsl/"), "{name}");
            assert!(text.contains("(Apache-2.0, see NOTICE)"), "{name}");
            if name.starts_with("noise_") || name == "color" {
                assert!(text.contains("Sony Pictures Imageworks") && text.contains("BSD-3-Clause"), "{name}");
            }
        }
    }

    fn op_dag(ops: &[(Op, bool)]) -> Dag {
        let mut dag = Dag::new("m");
        let c =
            dag.add(crate::Node { kind: NodeKind::Const(Value::Float(0.5)), ty: Type::Float, origin: String::new() });
        for (op, live) in ops {
            let kind = NodeKind::Op { op: *op, args: vec![c] };
            let n = dag.add(crate::Node { kind, ty: Type::Float, origin: String::new() });
            if *live {
                dag.surface.fields.push(SurfaceField { name: format!("f{}", n.0), ty: Type::Float, node: n });
            }
        }
        dag
    }

    #[test]
    fn names_follow_dependency_order_once_each() {
        let dag =
            op_dag(&[(Op::WorleyNoise2d, true), (Op::HsvToRgb, true), (Op::Fractal3d, true), (Op::CellNoise3d, true)]);
        assert_eq!(
            names_for(&dag),
            ["color", "noise_basics", "noise_hash", "noise_perlin", "noise_fractal", "noise_cell", "noise_worley"]
        );
    }

    #[test]
    fn dead_nodes_pull_in_nothing() {
        let dag = op_dag(&[(Op::Noise2d, false), (Op::NormalMap, true), (Op::RgbToHsv, false), (Op::Luminance, true)]);
        assert_eq!(names_for(&dag), ["normal"]);
        assert!(names_for(&op_dag(&[(Op::Add, true)])).is_empty());
    }
}
