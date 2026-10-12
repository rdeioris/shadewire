//! Parity cases: each is its own DAG of `Const` arguments feeding one op (a few chain two).

mod channel;
mod color;
mod hextile;
mod logic;
mod math;
mod noise;
mod normal;
mod trig;
mod vector;

use shadewire::{Dag, Node, NodeId, NodeKind, Op, SurfaceField, Type, Value};

/// Tolerance for exact-arithmetic families: math, vector, logic, channel.
pub const EXACT: f32 = 1e-5;
/// Tolerance for transcendental ops (trig, pow, exp, ln), noise and hextile.
pub const TRANSCENDENTAL: f32 = 1e-4;

pub struct Case {
    /// `family/op ...`: the family is the text before the first `/`.
    pub name: String,
    pub dag: Dag,
    pub node: NodeId,
    /// Relative-or-absolute: `|gpu - cpu| <= tolerance * max(1, |cpu|)`.
    pub tolerance: f32,
}

pub fn all() -> Vec<Case> {
    let mut cases = Vec::new();
    cases.extend(math::cases());
    cases.extend(trig::cases());
    cases.extend(vector::cases());
    cases.extend(logic::cases());
    cases.extend(channel::cases());
    cases.extend(color::cases());
    cases.extend(noise::cases());
    cases.extend(normal::cases());
    cases.extend(hextile::cases());
    cases
}

/// The family of a case name.
pub fn family(name: &str) -> &str {
    name.split('/').next().unwrap_or(name)
}

/// A DAG holding `inputs` as constants and `op` over them, the op made live as a surface field.
pub fn op_case(family: &str, op: Op, ty: Type, inputs: &[Value], tolerance: f32) -> Case {
    let mut dag = Dag::new("m");
    let args: Vec<NodeId> = inputs.iter().map(|v| constant(&mut dag, v)).collect();
    let node = dag.add(Node { kind: NodeKind::Op { op, args }, ty, origin: String::new() });
    dag.surface.fields.push(SurfaceField { name: "out".into(), ty, node });
    Case { name: format!("{family}/{op:?} -> {ty:?} {}", show(inputs)), dag, node, tolerance }
}

pub fn unary(family: &str, op: Op, ty: Type, a: Value, tolerance: f32) -> Case {
    op_case(family, op, ty, &[a], tolerance)
}

pub fn binary(family: &str, op: Op, ty: Type, a: Value, b: Value, tolerance: f32) -> Case {
    op_case(family, op, ty, &[a, b], tolerance)
}

pub fn ternary(family: &str, op: Op, ty: Type, a: Value, b: Value, c: Value, tolerance: f32) -> Case {
    op_case(family, op, ty, &[a, b, c], tolerance)
}

/// `second(first(input))`: for round trips.
pub fn chain(family: &str, first: Op, second: Op, ty: Type, input: Value, tolerance: f32) -> Case {
    let mut dag = Dag::new("m");
    let a = constant(&mut dag, &input);
    let mid = dag.add(Node { kind: NodeKind::Op { op: first, args: vec![a] }, ty, origin: String::new() });
    let node = dag.add(Node { kind: NodeKind::Op { op: second, args: vec![mid] }, ty, origin: String::new() });
    dag.surface.fields.push(SurfaceField { name: "out".into(), ty, node });
    Case { name: format!("{family}/{second:?}({first:?}) -> {ty:?} {}", show(&[input])), dag, node, tolerance }
}

fn constant(dag: &mut Dag, value: &Value) -> NodeId {
    dag.add(Node { kind: NodeKind::Const(value.clone()), ty: value.ty(), origin: String::new() })
}

fn show(inputs: &[Value]) -> String {
    let parts: Vec<String> = inputs.iter().map(|v| format!("{v:?}")).collect();
    format!("[{}]", parts.join(", "))
}

/// `[v[i], v[i+1], v[i+2]]` wrapping: color3 inputs built from a scalar list.
pub fn triple(v: &[f32], i: usize) -> Value {
    Value::Color3([v[i % v.len()], v[(i + 1) % v.len()], v[(i + 2) % v.len()]])
}
