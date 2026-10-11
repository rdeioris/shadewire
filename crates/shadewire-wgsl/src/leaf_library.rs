//! The WGSL leaf library: ports of the MaterialX genglsl helpers that the color, noise
//! and normal ops call. A module includes only the files its live nodes need.

use shadewire_ir::reflection::live::live_nodes;
use shadewire_ir::{Dag, NodeKind, Op};

/// Every library file, in dependency order.
pub const FILES: [&str; 8] =
    ["color", "normal", "noise_basics", "noise_hash", "noise_perlin", "noise_fractal", "noise_cell", "noise_worley"];

/// The library sources a DAG needs, each once, in dependency order.
pub fn files_for(dag: &Dag) -> Vec<&'static str> {
    names_for(dag).into_iter().map(source).collect()
}

/// The names of the library files a DAG needs, each once, in dependency order.
/// Only live nodes count, with the emitter's liveness.
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

/// The library files `op` calls into, with their dependencies.
pub fn needs(op: &Op) -> &'static [&'static str] {
    match op {
        Op::RgbToHsv | Op::HsvToRgb => &["color"],
        Op::NormalMap => &["normal"],
        Op::Noise2d | Op::Noise3d => &["noise_basics", "noise_hash", "noise_perlin"],
        Op::Fractal3d => &["noise_basics", "noise_hash", "noise_perlin", "noise_fractal"],
        Op::CellNoise2d | Op::CellNoise3d => &["noise_basics", "noise_hash", "noise_cell"],
        Op::WorleyNoise2d | Op::WorleyNoise3d => &["noise_basics", "noise_hash", "noise_cell", "noise_worley"],
        _ => &[],
    }
}

/// The WGSL text of one library file. Panics on an unknown name.
pub fn source(name: &str) -> &'static str {
    match name {
        "color" => include_str!("../lib/color.wgsl"),
        "normal" => include_str!("../lib/normal.wgsl"),
        "noise_basics" => include_str!("../lib/noise_basics.wgsl"),
        "noise_hash" => include_str!("../lib/noise_hash.wgsl"),
        "noise_perlin" => include_str!("../lib/noise_perlin.wgsl"),
        "noise_fractal" => include_str!("../lib/noise_fractal.wgsl"),
        "noise_cell" => include_str!("../lib/noise_cell.wgsl"),
        "noise_worley" => include_str!("../lib/noise_worley.wgsl"),
        other => panic!("no leaf library file {other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_files_validate() {
        for set in [
            vec!["color"],
            vec!["noise_basics", "noise_hash", "noise_perlin"],
            vec!["noise_basics", "noise_hash", "noise_cell"],
            vec!["noise_basics", "noise_hash", "noise_cell", "noise_worley"],
            vec!["noise_basics", "noise_hash", "noise_perlin", "noise_fractal"],
            vec!["normal"],
        ] {
            let text: String = set.iter().map(|f| source(f)).collect::<Vec<_>>().join("\n");
            let module =
                naga::front::wgsl::parse_str(&text).unwrap_or_else(|e| panic!("{set:?}: {}", e.emit_to_string(&text)));
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
                .validate(&module)
                .unwrap_or_else(|e| panic!("{set:?}: {e:?}"));
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

    /// The hash files use `bitcast<u32>` for Rust's `as u32` on an i32; WGSL's `u32(i32)`
    /// is the same bit reinterpretation (naga 30 cannot const-evaluate `bitcast`, so that half
    /// rests on the WGSL spec). `i32(f32)` truncates like `as i32`, but saturates high at
    /// 2147483520 (the largest f32 below 2^31) where Rust gives i32::MAX: mx_floor agrees only
    /// below 2^31 in magnitude, where every f32 is already an integer.
    #[test]
    fn wgsl_integer_conversions_against_rust_as() {
        let text = "
const_assert u32(-7i) == 4294967289u;
const_assert i32(-2.5f) == -2i;
const_assert i32(2147483520.0f) == 2147483520i;
const_assert i32(3.0e9f) == 2147483520i;
const_assert i32(-3.0e9f) == -2147483647i - 1i;
";
        assert_eq!(-7i32 as u32, 4294967289);
        assert_eq!((-2.5f32 as i32, 2147483520.0f32 as i32), (-2, 2147483520));
        assert_eq!((3.0e9f32 as i32, -3.0e9f32 as i32), (i32::MAX, i32::MIN));
        naga::front::wgsl::parse_str(text).unwrap_or_else(|e| panic!("{}", e.emit_to_string(text)));
    }

    fn op_dag(ops: &[(Op, bool)]) -> Dag {
        use crate::emit::test_support::add;
        use shadewire_ir::{SurfaceField, Type, Value};
        let mut dag = Dag::new("m");
        let c = add(&mut dag, NodeKind::Const(Value::Float(0.5)), Type::Float);
        for (op, live) in ops {
            let n = add(&mut dag, NodeKind::Op { op: *op, args: vec![c] }, Type::Float);
            if *live {
                dag.surface.fields.push(SurfaceField { name: format!("f{}", n.0), ty: Type::Float, node: n });
            }
        }
        dag
    }

    #[test]
    fn names_follow_dependency_order_once_each() {
        let dag = op_dag(&[(Op::WorleyNoise2d, true), (Op::HsvToRgb, true), (Op::Fractal3d, true), (Op::CellNoise3d, true)]);
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

    #[test]
    fn files_are_the_sources_of_the_names() {
        let dag = op_dag(&[(Op::CellNoise2d, true)]);
        assert_eq!(files_for(&dag), [source("noise_basics"), source("noise_hash"), source("noise_cell")]);
    }
}
