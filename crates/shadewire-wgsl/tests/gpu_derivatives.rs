//! Host derivatives on the GPU: for every parity case of an op with a derivative rule, the derivative the
//! generated code carries (its constant arguments moving along a fixed direction) must match a central finite
//! difference of the interpreter's value.

mod gpu;

use gpu::cases::{Case, all, family};
use shadewire::interpret::{ContextSource, NoTextures, eval_nodes};
use shadewire::shader::material::derivative::differentiable;
use shadewire::shader::material::derivative_probe;
use shadewire::shader::{Item, Module, library};
use shadewire::{ContextRead, Dag, NodeId, NodeKind, Op, Space, TransformKind, Value};

struct NoContext;

impl ContextSource for NoContext {
    fn read(&mut self, read: &ContextRead) -> Value {
        panic!("derivative case read {read:?}")
    }
    fn transform(&mut self, kind: TransformKind, _: Space, _: Space, _: [f32; 3]) -> [f32; 3] {
        panic!("derivative case transformed a {kind:?}")
    }
}

/// Ops whose rule is zero on purpose though the op varies (noise, colour space changes, normal maps, hex tiling),
/// or that read no floats: their cases are not compared.
fn compared(op: &Op) -> bool {
    !matches!(
        op,
        Op::Noise2d
            | Op::Noise3d
            | Op::Fractal2d
            | Op::Fractal3d
            | Op::CellNoise2d
            | Op::CellNoise3d
            | Op::WorleyNoise2d
            | Op::WorleyNoise3d
            | Op::RgbToHsv
            | Op::HsvToRgb
            | Op::NormalMap
            | Op::HextileCoord(_)
            | Op::HextileGrad(_)
            | Op::HextileWeights
            | Op::Dpdx
            | Op::Dpdy
            | Op::Transform { .. }
            | Op::BitwiseAnd
            | Op::BitwiseOr
            | Op::BitwiseXor
            | Op::ShiftLeft
            | Op::ShiftRight
            | Op::ShiftRightUnsigned
    )
}

/// The ops a case's node reads, itself included.
fn ops(dag: &Dag, node: NodeId) -> Vec<Op> {
    let mut out = Vec::new();
    let mut stack = vec![node];
    while let Some(id) = stack.pop() {
        if let NodeKind::Op { op, .. } = &dag.node(id).kind {
            out.push(*op);
        }
        stack.extend(dag.args(id));
    }
    out
}

/// A fixed direction for each float constant the case reads: per component, a different small step.
fn seeds(dag: &Dag, node: NodeId) -> Vec<(NodeId, Value)> {
    let mut out = Vec::new();
    let mut seen = vec![false; dag.nodes.len()];
    let mut stack = vec![node];
    while let Some(id) = stack.pop() {
        if seen[id.index()] {
            continue;
        }
        seen[id.index()] = true;
        let n = dag.node(id);
        // A selecting op's condition stays put: its rule takes the branch chosen, which a moving condition changes.
        let kept: &[usize] = match &n.kind {
            NodeKind::Op { op: Op::IfGreater | Op::IfGreaterEq | Op::IfEqual, .. } => &[0, 1],
            NodeKind::Op { op: Op::Switch, .. } => &[10],
            _ => &[],
        };
        if let NodeKind::Const(v) = &n.kind
            && differentiable(n.ty)
        {
            let k = out.len() as f32;
            let floats: Vec<f32> = (0..v.floats().len()).map(|c| 0.7 - 0.37 * (k + c as f32) % 1.3).collect();
            out.push((id, Value::from_floats(n.ty, &floats)));
        }
        stack.extend(dag.args(id).into_iter().enumerate().filter(|(k, _)| !kept.contains(k)).map(|(_, a)| a));
    }
    out
}

/// The case node's value with every seed moved by `t` times its direction.
fn value_at(dag: &Dag, node: NodeId, seeds: &[(NodeId, Value)], t: f32) -> Vec<f32> {
    let mut moved = dag.clone();
    for (id, d) in seeds {
        let NodeKind::Const(v) = &dag.node(*id).kind else { unreachable!() };
        let floats: Vec<f32> = v.floats().iter().zip(d.floats()).map(|(x, dx)| x + t * dx).collect();
        moved.nodes[id.index()].kind = NodeKind::Const(Value::from_floats(v.ty(), &floats));
    }
    eval_nodes(&moved, &mut NoContext, &NoTextures)[node.index()].floats()
}

/// A central difference, or None where the value is not smooth at the case's point: its forward and backward
/// differences disagree (a kink: a tie of min or max, a clamp's edge; or a step), or its central differences over
/// two step sizes do (a symmetric jump, as sign at 0, or a vertical slope, as a root at 0), or a value is not
/// finite.
fn finite_difference(dag: &Dag, node: NodeId, seeds: &[(NodeId, Value)]) -> Option<[f32; 4]> {
    let (h, wide) = (5e-4, 2e-3);
    let at = value_at(dag, node, seeds, 0.0);
    let (ahead, behind) = (value_at(dag, node, seeds, h), value_at(dag, node, seeds, -h));
    let (far_ahead, far_behind) = (value_at(dag, node, seeds, wide), value_at(dag, node, seeds, -wide));
    let apart = |a: f32, b: f32| (a - b).abs() > 0.02 * a.abs().max(b.abs()).max(1.0);
    let mut out = [0.0; 4];
    for k in 0..at.len().min(4) {
        let (forward, backward) = ((ahead[k] - at[k]) / h, (at[k] - behind[k]) / h);
        let (central, far) = ((ahead[k] - behind[k]) / (2.0 * h), (far_ahead[k] - far_behind[k]) / (2.0 * wide));
        if ![forward, backward, central, far].iter().all(|v| v.is_finite())
            || apart(forward, backward)
            || apart(central, far)
        {
            return None;
        }
        out[k] = central;
    }
    Some(out)
}

fn wgsl_functions(cases: &[(&Case, Vec<(NodeId, Value)>)]) -> String {
    let mut names: Vec<&str> = Vec::new();
    for (case, _) in cases {
        names.extend(library::names_for(&case.dag));
    }
    let items = library::FILES.iter().filter(|n| names.contains(n)).flat_map(|n| library::module(n).items.clone());
    let mut text: Vec<String> = vec![shadewire_wgsl::print(&Module { items: items.collect() })];
    for (i, (case, seeds)) in cases.iter().enumerate() {
        let function = derivative_probe(&case.dag, case.node, seeds, &format!("case_{i}"));
        let probe = shadewire_wgsl::print(&Module { items: vec![Item::Function(function)] });
        text.push(gpu::opaque_constants(&case.dag, case.node, &probe));
    }
    text.join("\n")
}

/// A case compared: its seeds and the finite difference.
type Chosen<'a> = (&'a Case, Vec<(NodeId, Value)>, [f32; 4]);

/// The worst error allowed: the finite differences' own, or the case's when larger.
const ALLOWED: f32 = 0.02;

/// Opt-in, as the parity test: a GPU and minutes of driver compile.
#[test]
#[ignore = "gpu derivatives: cargo test -p shadewire-wgsl --test gpu_derivatives -- --ignored"]
fn carried_derivatives_match_finite_differences() {
    let cases = all();
    let mut chosen: Vec<Chosen> = Vec::new();
    let mut skipped = 0;
    for case in &cases {
        if !differentiable(case.dag.node(case.node).ty) || !ops(&case.dag, case.node).iter().all(compared) {
            continue;
        }
        let s = seeds(&case.dag, case.node);
        if s.is_empty() {
            continue;
        }
        match finite_difference(&case.dag, case.node, &s) {
            Some(fd) => chosen.push((case, s, fd)),
            None => skipped += 1,
        }
    }
    let mut failures = 0;
    let mut compared_count = 0;
    let mut start = 0;
    while start < chosen.len() {
        let fam = family(&chosen[start].0.name);
        let end = chosen[start..].iter().position(|c| family(&c.0.name) != fam).map_or(chosen.len(), |p| start + p);
        let chunk: Vec<(&Case, Vec<(NodeId, Value)>)> =
            chosen[start..end].iter().map(|(c, s, _)| (*c, s.clone())).collect();
        let functions = wgsl_functions(&chunk);
        let Some(gpu) = gpu::run_probes(&functions, chunk.len()) else {
            return;
        };
        for ((case, _, fd), g) in chosen[start..end].iter().zip(gpu) {
            compared_count += 1;
            let worst = g.iter().zip(fd).map(|(a, b)| (a - b).abs() / b.abs().max(1.0)).fold(0.0f32, f32::max);
            if worst.is_nan() || worst > ALLOWED.max(case.tolerance) {
                failures += 1;
                println!("MISMATCH {}\n    carried {g:?}\n    finite  {fd:?}\n    error {worst:e}", case.name);
            }
        }
        start = end;
    }
    println!("{compared_count} cases compared, {skipped} skipped where the value is not smooth");
    assert_eq!(failures, 0, "{failures} carried derivatives differ from finite differences");
}
