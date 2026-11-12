//! Host derivatives (`CompileOptions::host_derivatives`): the screen-space derivatives of every node a texture
//! coordinate depends on, carried through the graph by the chain rule from the host's derivatives of the context
//! reads (`shadewire_host_<read>_dx` / `_dy`). Samples then use `textureSampleGrad` with their coordinates'
//! derivatives, and `dpdx` / `dpdy` nodes return them. For hosts whose pixels do not share a triangle with their
//! 2 × 2 quad, as a visibility buffer's full-screen shading does, where the hardware's derivatives cross triangles.

use super::ops::{equal_expr, op};
use super::values::*;
use crate::shader::ast::*;
use crate::{
    ContextClass, ContextEntry, ContextRead, Dag, HOST_PREFIX, NodeId, NodeKind, Op, Reflection, TransformKind, Type,
    transform_name,
};

/// Screen x or screen y.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

impl Axis {
    pub fn suffix(self) -> &'static str {
        match self {
            Axis::X => "_dx",
            Axis::Y => "_dy",
        }
    }
}

/// Whether values of `ty` carry derivatives: floats, vectors and colours.
pub fn differentiable(ty: Type) -> bool {
    matches!(ty, Type::Float | Type::Vector2 | Type::Vector3 | Type::Vector4 | Type::Color3 | Type::Color4)
}

/// The arguments whose derivatives an op's derivative reads: none for the ops whose derivative is zero, the two
/// branches of the selecting ops, all of them otherwise.
fn differentiated_args(o: &Op, count: usize) -> Vec<usize> {
    match o {
        Op::Sign
        | Op::Floor
        | Op::Ceil
        | Op::Round
        | Op::Greater
        | Op::GreaterEq
        | Op::Equal
        | Op::And
        | Op::Or
        | Op::Not
        | Op::RgbToHsv
        | Op::HsvToRgb
        | Op::Noise2d
        | Op::Noise3d
        | Op::Fractal2d
        | Op::Fractal3d
        | Op::CellNoise2d
        | Op::CellNoise3d
        | Op::WorleyNoise2d
        | Op::WorleyNoise3d
        | Op::NormalMap
        | Op::HextileCoord(_)
        | Op::HextileGrad(_)
        | Op::HextileWeights
        | Op::Dpdx
        | Op::Dpdy
        | Op::BitwiseAnd
        | Op::BitwiseOr
        | Op::BitwiseXor
        | Op::ShiftLeft
        | Op::ShiftRight
        | Op::ShiftRightUnsigned => Vec::new(),
        Op::IfGreater | Op::IfGreaterEq | Op::IfEqual => vec![2, 3],
        Op::Switch => (0..10).collect(),
        _ => (0..count).collect(),
    }
}

/// The nodes whose derivatives the code needs: the live nodes the coordinates of a sample without explicit
/// gradients, or the argument of a `Dpdx` / `Dpdy` node, depend on, through ops and up to the context reads.
/// Constants, uniforms and samples (a dependent read) are left out: their derivatives are zero.
pub fn cone(dag: &Dag, live: &[bool]) -> Vec<bool> {
    let mut needed = vec![false; dag.nodes.len()];
    let mut stack = Vec::new();
    for (i, n) in dag.nodes.iter().enumerate() {
        if !live[i] {
            continue;
        }
        match &n.kind {
            NodeKind::Sample { uv, grad: None, .. } => stack.push(*uv),
            NodeKind::Op { op: Op::Dpdx | Op::Dpdy, args } => stack.push(args[0]),
            _ => {}
        }
    }
    while let Some(id) = stack.pop() {
        let n = dag.node(id);
        if needed[id.index()] || !differentiable(n.ty) {
            continue;
        }
        match &n.kind {
            NodeKind::Context(read) if read.class() != ContextClass::Global => needed[id.index()] = true,
            NodeKind::Op { op: o, args } => {
                needed[id.index()] = true;
                stack.extend(differentiated_args(o, args.len()).into_iter().map(|k| args[k]));
            }
            _ => {}
        }
    }
    needed
}

/// What the derivative code calls besides the value code, added to `reflection`: each differentiated context read
/// in `context.derivatives`, and in `context.transforms` the vector transform a differentiated point transform's
/// derivative moves by.
pub fn reflect(dag: &Dag, reflection: &mut Reflection) {
    let carried = cone(dag, &crate::reflection::live::live_nodes(dag));
    let mut derivatives: Vec<ContextEntry> = Vec::new();
    let mut transforms = reflection.context.transforms.clone();
    for n in dag.nodes.iter().enumerate().filter(|(i, _)| carried[*i]).map(|(_, n)| n) {
        match &n.kind {
            NodeKind::Context(read) => {
                let name = read_name(read, reflection);
                if !derivatives.iter().any(|e| e.name == name) {
                    derivatives.push(ContextEntry { name, ty: read.ty() });
                }
            }
            NodeKind::Op { op: Op::Transform { kind: TransformKind::Point, from, to }, .. } => {
                transforms.push(transform_name(TransformKind::Vector, *from, *to));
            }
            _ => {}
        }
    }
    derivatives.sort_by(|a, b| a.name.cmp(&b.name));
    transforms.sort();
    transforms.dedup();
    reflection.context.derivatives = derivatives;
    reflection.context.transforms = transforms;
}

/// The host function name (after `shadewire_host_`) a context read's derivatives append `_dx` / `_dy` to: the
/// reflection name, or a geomprop's function without the prefix.
pub fn read_name(read: &ContextRead, reflection: &Reflection) -> String {
    match read {
        ContextRead::GeomProp { name, .. } => {
            let g = reflection.context.geomprops.iter().find(|g| g.name == *name).expect("live geomprop is reflected");
            g.function.strip_prefix(HOST_PREFIX).unwrap_or(&g.function).to_string()
        }
        other => other.reflection_name(),
    }
}

/// Node `id`'s derivative along `axis`: its let when it has one, else zero of its (float) type.
pub fn dnode(dag: &Dag, cone: &[bool], id: NodeId, axis: Axis) -> Expr {
    let ty = dag.node(id).ty;
    if cone[id.index()] {
        Expr::name(format!("n{}{}", id.0, axis.suffix()), Ty::of(ty))
    } else if differentiable(ty) {
        cast(Expr::f32(0.0), &Ty::of(ty))
    } else {
        Expr::f32(0.0)
    }
}

/// The expression of node `id`'s derivative along `axis`, of its type. The node is in the cone: an op or a
/// varying, derived or geomprop context read (which needs `reflection`).
pub fn derivative(dag: &Dag, reflection: Option<&Reflection>, cone: &[bool], id: NodeId, axis: Axis) -> Expr {
    let n = dag.node(id);
    let ty = Ty::of(n.ty);
    match &n.kind {
        NodeKind::Context(read) => {
            let reflection = reflection.expect("a context read's derivative needs the reflection");
            let name = format!("{HOST_PREFIX}{}{}", read_name(read, reflection), axis.suffix());
            cast(Expr::call(name, Vec::new(), Ty::of(read.ty())), &ty)
        }
        NodeKind::Op { op: o, args } => {
            let values: Vec<Expr> = args.iter().map(|a| node(*a, Ty::of(dag.node(*a).ty))).collect();
            let ders: Vec<Expr> = args.iter().map(|a| dnode(dag, cone, *a, axis)).collect();
            op_derivative(o, &values, &ders, &ty)
        }
        other => panic!("{other:?} has no derivative let"),
    }
}

fn zero(ty: &Ty) -> Expr {
    cast(Expr::f32(0.0), ty)
}

fn neg(e: Expr) -> Expr {
    Expr::unary(UnaryOp::Neg, e)
}

fn lt(a: Expr, b: Expr) -> Expr {
    bin(BinaryOp::Lt, a, b)
}

fn le(a: Expr, b: Expr) -> Expr {
    bin(BinaryOp::Le, a, b)
}

fn gt(a: Expr, b: Expr) -> Expr {
    bin(BinaryOp::Gt, a, b)
}

fn ge(a: Expr, b: Expr) -> Expr {
    bin(BinaryOp::Ge, a, b)
}

fn call1(b: Builtin, a: Expr) -> Expr {
    builtin(b, vec![a])
}

/// The derivative of `op` (as `ops::op` computes it) over arguments `a` with derivatives `d`, of the node type
/// `ty`. Zero for the ops `differentiated_args` lists none for.
pub fn op_derivative(o: &Op, a: &[Expr], d: &[Expr], ty: &Ty) -> Expr {
    if differentiated_args(o, a.len()).is_empty() {
        return zero(ty);
    }
    match o {
        Op::Add
        | Op::Subtract
        | Op::Multiply
        | Op::Divide
        | Op::Modulo
        | Op::Invert
        | Op::Absval
        | Op::Fract
        | Op::Power
        | Op::SafePower
        | Op::Sqrt
        | Op::Exp
        | Op::Ln
        | Op::Min
        | Op::Max
        | Op::Clamp
        | Op::Mix
        | Op::Smoothstep
        | Op::Remap => math(o, a, d, ty),
        Op::Sin | Op::Cos | Op::Tan | Op::Asin | Op::Acos | Op::Atan2 => trig(o, a, d, ty),
        Op::Normalize | Op::Magnitude | Op::Distance | Op::DotProduct | Op::CrossProduct | Op::Rotate2d => {
            vector(o, a, d, ty)
        }
        Op::Transform { kind, from, to } => {
            let v = cast(a[0].clone(), &Ty::vec(3));
            let dv = cast(d[0].clone(), &Ty::vec(3));
            let moved = match kind {
                // A point's derivative is a vector: it moves without the translation.
                TransformKind::Point | TransformKind::Vector => {
                    let host = format!("{HOST_PREFIX}{}", transform_name(TransformKind::Vector, *from, *to));
                    Expr::call(host, vec![dv], Ty::vec(3))
                }
                // normalize(M v): the normalisation's derivative of M dv.
                TransformKind::Normal => {
                    let host = format!("{HOST_PREFIX}{}", transform_name(TransformKind::Normal, *from, *to));
                    let u = Expr::call(host.clone(), vec![v], Ty::vec(3));
                    let du = Expr::call(host, vec![dv], Ty::vec(3));
                    normalized_derivative(u, du)
                }
            };
            cast(moved, ty)
        }
        Op::IfGreater | Op::IfGreaterEq | Op::IfEqual => {
            let cond = match o {
                Op::IfGreater => gt(first_f32(&a[0]), first_f32(&a[1])),
                Op::IfGreaterEq => ge(first_f32(&a[0]), first_f32(&a[1])),
                _ => equal_expr(&a[0], &a[1]),
            };
            select(cast(d[3].clone(), ty), cast(d[2].clone(), ty), cond)
        }
        Op::Switch => {
            let which = first_f32(&a[10]);
            let mut out = zero(ty);
            for k in (1..=10).rev() {
                out = select(out, cast(d[k - 1].clone(), ty), lt(which.clone(), Expr::f32(k as f32)));
            }
            out
        }
        // Linear, with no constant part: the op itself on the derivatives.
        Op::Extract(_) | Op::Combine => op(o, d, ty),
        Op::Convert => {
            // As `ops::channel`, but the channels a narrower input gains are 1.0, whose derivative is 0.
            let have = a[0].ty.width().unwrap_or(1);
            let want = ty.width().unwrap_or(1);
            if have == 1 || have == want || have > want {
                op(o, d, ty)
            } else {
                let mut parts = vec![d[0].clone()];
                parts.extend((have..want).map(|_| Expr::f32(0.0)));
                Expr::construct(ty.clone(), parts)
            }
        }
        Op::Luminance => {
            let at = |e: &Expr, i: i32| e.clone().index(Expr::i32(i));
            let (c, k, dc, dk) = (&a[0], &a[1], &d[0], &d[1]);
            let term = |i: i32| add(mul(at(dc, i), at(k, i)), mul(at(c, i), at(dk, i)));
            let sum = add(add(term(0), term(1)), term(2));
            let rgb = Expr::construct(Ty::vec(3), vec![sum]);
            if *ty == Ty::vec(4) && a[0].ty == Ty::vec(4) {
                Expr::construct(Ty::vec(4), vec![rgb, at(dc, 3)])
            } else {
                cast(rgb, ty)
            }
        }
        other => panic!("{other:?} has no derivative rule"),
    }
}

/// The derivative of `normalize(u)` along `du`: the part of `du` across `n`, over `|u|`.
fn normalized_derivative(u: Expr, du: Expr) -> Expr {
    let n = call1(Builtin::Normalize, u.clone());
    let along = mul(n.clone(), builtin(Builtin::Dot, vec![n, du.clone()]));
    div(sub(du, along), builtin(Builtin::Max, vec![call1(Builtin::Length, u), Expr::f32(1e-20)]))
}

/// A power's exponent, widened as `math` widens it.
fn y_of(args: &[Expr], ty: &Ty) -> Expr {
    wide(args, ty)[1].clone()
}

fn math(o: &Op, args: &[Expr], ders: &[Expr], ty: &Ty) -> Expr {
    let work = work_type(ty);
    let a = wide(args, ty);
    let d = wide(ders, ty);
    let z = || zero(&work);
    let expr = match o {
        Op::Add => add(d[0].clone(), d[1].clone()),
        Op::Subtract => sub(d[0].clone(), d[1].clone()),
        Op::Multiply => add(mul(d[0].clone(), a[1].clone()), mul(a[0].clone(), d[1].clone())),
        Op::Divide => {
            div(sub(mul(d[0].clone(), a[1].clone()), mul(a[0].clone(), d[1].clone())), mul(a[1].clone(), a[1].clone()))
        }
        Op::Modulo => sub(d[0].clone(), mul(d[1].clone(), call1(Builtin::Floor, div(a[0].clone(), a[1].clone())))),
        Op::Invert => sub(d[1].clone(), d[0].clone()),
        Op::Absval => mul(call1(Builtin::Sign, a[0].clone()), d[0].clone()),
        // A tile's coordinates move with their input; the jump at a tile's edge is not differentiated.
        Op::Fract => d[0].clone(),
        Op::Power => {
            let (x, y) = (a[0].clone(), a[1].clone());
            let pow = builtin(Builtin::Pow, vec![x.clone(), y.clone()]);
            let base = mul(
                mul(y.clone(), builtin(Builtin::Pow, vec![x.clone(), sub(y, cast(Expr::f32(1.0), &work))])),
                d[0].clone(),
            );
            let exponent = mul(mul(pow, call1(Builtin::Log, x.clone())), d[1].clone());
            // The base's term also at 0 when the exponent is at least 1 (x¹'s slope is 1 there); the exponent's
            // needs log x.
            let one = cast(Expr::f32(1.0), &work);
            let base_defined = bin(
                BinaryOp::BitOr,
                gt(x.clone(), z()),
                bin(BinaryOp::BitAnd, bin(BinaryOp::Eq, x.clone(), z()), ge(y_of(args, ty), one)),
            );
            add(select(z(), base, base_defined), select(z(), exponent, gt(x, z())))
        }
        Op::SafePower => {
            let (x, y) = (a[0].clone(), a[1].clone());
            let m = call1(Builtin::Abs, x.clone());
            let base = mul(
                mul(y.clone(), builtin(Builtin::Pow, vec![m.clone(), sub(y.clone(), cast(Expr::f32(1.0), &work))])),
                d[0].clone(),
            );
            let exponent = mul(
                mul(
                    mul(call1(Builtin::Sign, x), builtin(Builtin::Pow, vec![m.clone(), y])),
                    call1(Builtin::Log, m.clone()),
                ),
                d[1].clone(),
            );
            let one = cast(Expr::f32(1.0), &work);
            let base_defined = bin(BinaryOp::BitOr, gt(m.clone(), z()), ge(y_of(args, ty), one));
            add(select(z(), base, base_defined), select(z(), exponent, gt(m, z())))
        }
        Op::Sqrt => select(
            z(),
            div(d[0].clone(), mul(cast(Expr::f32(2.0), &work), call1(Builtin::Sqrt, a[0].clone()))),
            gt(a[0].clone(), z()),
        ),
        Op::Exp => mul(call1(Builtin::Exp, a[0].clone()), d[0].clone()),
        Op::Ln => div(d[0].clone(), a[0].clone()),
        Op::Min => select(d[1].clone(), d[0].clone(), le(a[0].clone(), a[1].clone())),
        Op::Max => select(d[1].clone(), d[0].clone(), ge(a[0].clone(), a[1].clone())),
        Op::Clamp => {
            let inner = builtin(Builtin::Max, vec![a[0].clone(), a[1].clone()]);
            let dinner = select(d[1].clone(), d[0].clone(), ge(a[0].clone(), a[1].clone()));
            select(d[2].clone(), dinner, le(inner, a[2].clone()))
        }
        Op::Mix => {
            // a1 + (a0 - a1) t, the amount a scalar or of the node's width.
            let (t, dt) = if args[2].ty.is_scalar() {
                (cast(args[2].clone(), &Ty::F32), cast(ders[2].clone(), &Ty::F32))
            } else {
                (a[2].clone(), d[2].clone())
            };
            add(add(d[1].clone(), mul(sub(d[0].clone(), d[1].clone()), t)), mul(sub(a[0].clone(), a[1].clone()), dt))
        }
        Op::Smoothstep => {
            let (x, lo, hi) = (a[0].clone(), a[1].clone(), a[2].clone());
            let (dx, dlo, dhi) = (d[0].clone(), d[1].clone(), d[2].clone());
            let range = sub(hi.clone(), lo.clone());
            let t = div(sub(x.clone(), lo.clone()), range.clone());
            let dt = div(
                sub(mul(sub(dx, dlo.clone()), range.clone()), mul(sub(x.clone(), lo.clone()), sub(dhi, dlo))),
                mul(range.clone(), range),
            );
            let one = cast(Expr::f32(1.0), &work);
            let inside = mul(mul(mul(cast(Expr::f32(6.0), &work), t.clone()), sub(one, t)), dt);
            select(select(inside, z(), le(x.clone(), lo)), z(), ge(x, hi))
        }
        Op::Remap => {
            let num = mul(sub(a[0].clone(), a[1].clone()), sub(a[4].clone(), a[3].clone()));
            let dnum = add(
                mul(sub(d[0].clone(), d[1].clone()), sub(a[4].clone(), a[3].clone())),
                mul(sub(a[0].clone(), a[1].clone()), sub(d[4].clone(), d[3].clone())),
            );
            let den = sub(a[2].clone(), a[1].clone());
            let dden = sub(d[2].clone(), d[1].clone());
            add(d[3].clone(), div(sub(mul(dnum, den.clone()), mul(num, dden)), mul(den.clone(), den)))
        }
        other => panic!("{other:?} is not a math op"),
    };
    cast(expr, ty)
}

fn trig(o: &Op, args: &[Expr], ders: &[Expr], ty: &Ty) -> Expr {
    let work = work_type(ty);
    let a = wide(args, ty);
    let d = wide(ders, ty);
    let tiny = || cast(Expr::f32(1e-20), &work);
    let one = || cast(Expr::f32(1.0), &work);
    let root = |x: Expr| call1(Builtin::Sqrt, builtin(Builtin::Max, vec![sub(one(), mul(x.clone(), x)), tiny()]));
    let expr = match o {
        Op::Sin => mul(call1(Builtin::Cos, a[0].clone()), d[0].clone()),
        Op::Cos => neg(mul(call1(Builtin::Sin, a[0].clone()), d[0].clone())),
        Op::Tan => {
            let c = call1(Builtin::Cos, a[0].clone());
            div(d[0].clone(), mul(c.clone(), c))
        }
        Op::Asin => div(d[0].clone(), root(a[0].clone())),
        Op::Acos => neg(div(d[0].clone(), root(a[0].clone()))),
        // atan2(y, x): (x dy − y dx) / (x² + y²).
        Op::Atan2 => {
            let (y, x) = (a[0].clone(), a[1].clone());
            let num = sub(mul(x.clone(), d[0].clone()), mul(y.clone(), d[1].clone()));
            div(num, builtin(Builtin::Max, vec![add(mul(x.clone(), x), mul(y.clone(), y)), tiny()]))
        }
        other => panic!("{other:?} is not a trig op"),
    };
    cast(expr, ty)
}

fn vector(o: &Op, args: &[Expr], ders: &[Expr], ty: &Ty) -> Expr {
    if *o == Op::Rotate2d {
        // rotate(v, θ°): rotate(dv, θ) plus dθ (π / 180) times the rotated vector turned a quarter, (r.y, −r.x).
        let v = leading(&args[0], 2);
        let dv = leading(&ders[0], 2);
        let amount = first_f32(&args[1]);
        let damount = first_f32(&ders[1]);
        let r = lib_call("shadewire_rotate_vector2", vec![v, amount.clone()]);
        let rd = lib_call("shadewire_rotate_vector2", vec![dv, amount]);
        let turned = Expr::construct(Ty::vec(2), vec![r.clone().swizzle("y"), neg(r.swizzle("x"))]);
        let k = mul(damount, Expr::f32(std::f32::consts::PI / 180.0));
        return cast(add(rd, mul(turned, k)), ty);
    }
    let operand = work_type(&args[0].ty);
    let a: Vec<Expr> = args.iter().map(|e| cast(e.clone(), &operand)).collect();
    let d: Vec<Expr> = ders.iter().map(|e| cast(e.clone(), &operand)).collect();
    let scalar = operand.is_scalar();
    let expr = match o {
        Op::Normalize if scalar => zero(&operand),
        Op::Normalize => normalized_derivative(a[0].clone(), d[0].clone()),
        Op::Magnitude if scalar => mul(call1(Builtin::Sign, a[0].clone()), d[0].clone()),
        Op::Magnitude => builtin(Builtin::Dot, vec![call1(Builtin::Normalize, a[0].clone()), d[0].clone()]),
        Op::Distance => {
            let diff = sub(a[0].clone(), a[1].clone());
            let ddiff = sub(d[0].clone(), d[1].clone());
            if scalar {
                mul(call1(Builtin::Sign, diff), ddiff)
            } else {
                builtin(Builtin::Dot, vec![call1(Builtin::Normalize, diff), ddiff])
            }
        }
        Op::DotProduct if scalar => add(mul(d[0].clone(), a[1].clone()), mul(a[0].clone(), d[1].clone())),
        Op::DotProduct => add(
            builtin(Builtin::Dot, vec![d[0].clone(), a[1].clone()]),
            builtin(Builtin::Dot, vec![a[0].clone(), d[1].clone()]),
        ),
        Op::CrossProduct => add(
            builtin(Builtin::Cross, vec![d[0].clone(), a[1].clone()]),
            builtin(Builtin::Cross, vec![a[0].clone(), d[1].clone()]),
        ),
        other => panic!("{other:?} is not a vector op"),
    };
    cast(cast(expr, &work_type(ty)), ty)
}
