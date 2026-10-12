//! The Godot leaf library: ports of the MaterialX genglsl helpers that the color, noise
//! and normal ops call, with the WGSL library's function names. A shader includes only the
//! files its live nodes need.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use shadewire::reflection::live::live_nodes;
use shadewire::{Dag, NodeKind, Op};

/// Every library file, in dependency order.
pub const FILES: [&str; 10] = [
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
];

/// The library sources a DAG needs, each once, in dependency order.
pub fn files_for(dag: &Dag) -> Vec<&'static str> {
    names_for(dag).into_iter().map(source).collect()
}

/// The names of the library files a DAG needs, each once, in dependency order.
/// Only live nodes count.
pub fn names_for(dag: &Dag) -> Vec<&'static str> {
    let live = live_nodes(dag);
    let mut wanted: Vec<&'static str> = Vec::new();
    for (i, node) in dag.nodes.iter().enumerate() {
        if !live[i] {
            continue;
        }
        if let NodeKind::Op { op, .. } = &node.kind {
            wanted.extend_from_slice(needs(op));
        }
    }
    FILES.into_iter().filter(|f| wanted.contains(f)).collect()
}

/// The library files `op` calls into, with their dependencies (the WGSL back end's table).
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

/// The text of one library file. Panics on an unknown name.
pub fn source(name: &str) -> &'static str {
    match name {
        "color" => include_str!("../lib/color.gdshaderinc"),
        "normal" => include_str!("../lib/normal.gdshaderinc"),
        "vector" => include_str!("../lib/vector.gdshaderinc"),
        "hextile" => include_str!("../lib/hextile.gdshaderinc"),
        "noise_basics" => include_str!("../lib/noise_basics.gdshaderinc"),
        "noise_hash" => include_str!("../lib/noise_hash.gdshaderinc"),
        "noise_perlin" => include_str!("../lib/noise_perlin.gdshaderinc"),
        "noise_fractal" => include_str!("../lib/noise_fractal.gdshaderinc"),
        "noise_cell" => include_str!("../lib/noise_cell.gdshaderinc"),
        "noise_worley" => include_str!("../lib/noise_worley.gdshaderinc"),
        other => panic!("no leaf library file {other:?}"),
    }
}

/// Every identifier the library declares or uses outside comments: function, parameter and
/// local names. Godot refuses a parameter or local that redeclares a global, so uniforms
/// stay clear of all of them, whichever files a shader includes.
pub fn identifiers() -> &'static BTreeSet<&'static str> {
    static IDENTIFIERS: OnceLock<BTreeSet<&'static str>> = OnceLock::new();
    IDENTIFIERS.get_or_init(|| {
        let mut out = BTreeSet::new();
        for name in FILES {
            for line in source(name).lines() {
                let code = line.split("//").next().unwrap_or("");
                for word in code.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
                    if word.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
                        out.insert(word);
                    }
                }
            }
        }
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shadewire::{Node, SurfaceField, Type, Value};

    fn op_dag(ops: &[(Op, bool)]) -> Dag {
        let mut dag = Dag::new("m");
        let c = dag.add(Node { kind: NodeKind::Const(Value::Float(0.5)), ty: Type::Float, origin: String::new() });
        for (op, live) in ops {
            let kind = NodeKind::Op { op: *op, args: vec![c] };
            let n = dag.add(Node { kind, ty: Type::Float, origin: String::new() });
            if *live {
                dag.surface.fields.push(SurfaceField { name: format!("f{}", n.0), ty: Type::Float, node: n });
            }
        }
        dag
    }

    /// The same files, in the same order, as the WGSL back end for every op.
    #[test]
    fn selection_matches_the_wgsl_back_end() {
        assert_eq!(FILES, shadewire_wgsl::leaf_library::FILES);
        let ops = [
            Op::RgbToHsv,
            Op::HsvToRgb,
            Op::NormalMap,
            Op::Rotate2d,
            Op::HextileCoord(0),
            Op::HextileGrad(1),
            Op::HextileWeights,
            Op::Noise2d,
            Op::Noise3d,
            Op::Fractal3d,
            Op::CellNoise2d,
            Op::CellNoise3d,
            Op::WorleyNoise2d,
            Op::WorleyNoise3d,
            Op::Add,
            Op::Luminance,
        ];
        for op in ops {
            assert_eq!(needs(&op), shadewire_wgsl::leaf_library::needs(&op), "{op:?}");
        }
    }

    /// Every function the WGSL library defines has a Godot port of the same name.
    #[test]
    fn every_wgsl_function_is_ported() {
        for name in FILES {
            let wgsl = shadewire_wgsl::leaf_library::source(name);
            let ported = source(name);
            for line in wgsl.lines() {
                let Some(rest) = line.strip_prefix("fn ") else { continue };
                let function = rest.split('(').next().unwrap();
                assert!(ported.contains(&format!(" {function}(")), "{name}: {function}");
            }
        }
    }

    #[test]
    fn every_file_carries_the_attribution() {
        for name in FILES {
            let text = source(name);
            assert!(text.starts_with("// Port of MaterialX libraries/stdlib/genglsl/"), "{name}");
            assert!(text.contains("(Apache-2.0, see NOTICE)"), "{name}");
            if name.starts_with("noise_") || name == "color" {
                assert!(text.contains("Sony Pictures Imageworks") && text.contains("BSD-3-Clause"), "{name}");
            }
        }
    }

    #[test]
    fn names_follow_dependency_order_once_each() {
        let dag =
            op_dag(&[(Op::WorleyNoise2d, true), (Op::HsvToRgb, true), (Op::Fractal3d, true), (Op::CellNoise3d, true)]);
        assert_eq!(
            names_for(&dag),
            ["color", "noise_basics", "noise_hash", "noise_perlin", "noise_fractal", "noise_cell", "noise_worley"]
        );
        let dead = op_dag(&[(Op::Noise2d, false), (Op::NormalMap, true)]);
        assert_eq!(files_for(&dead), [source("normal")]);
    }

    #[test]
    fn identifiers_skip_comments() {
        let ids = identifiers();
        assert!(ids.contains("shadewire_hsvtorgb") && ids.contains("jit") && ids.contains("MxHextileLayout"));
        // Only in comments: "Some big number for jitter > 1".
        assert!(!ids.contains("big"));
    }
}
