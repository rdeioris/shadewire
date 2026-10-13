//! One `let n{i}: T = ...;` per live node: constants and ops, uniform reads, host reads and texture samples.

use super::MaterialOptions;
use super::ops::op;
use super::values::*;
use crate::reflection::TextureReflection;
use crate::reflection::live::live_nodes;
use crate::shader::ast::*;
use crate::shader::names::UniformNames;
use crate::{AddressMode, ContextRead, Dag, NodeId, NodeKind, Op, Reflection, Value};

/// The uniform block's variable and struct.
pub const UNIFORMS: &str = "shadewire_u";
pub const UNIFORM_STRUCT: &str = "MaterialUniforms";

pub fn texture_name(index: usize) -> String {
    format!("shadewire_t{index}")
}

pub fn sampler_name(index: usize) -> String {
    format!("shadewire_s{index}")
}

/// One `let` per live node, in node order. Live uniforms are numbered in node order, exactly as reflection lists
/// them.
pub fn node_lets(dag: &Dag, reflection: &Reflection, options: &MaterialOptions) -> Vec<Stmt> {
    let live = live_nodes(dag);
    let members = UniformNames::new(reflection);
    let mut uniform_index = 0;
    let mut lines = Vec::new();
    for (i, n) in dag.nodes.iter().enumerate() {
        if !live[i] {
            continue;
        }
        let id = NodeId(i as u32);
        let ty = Ty::of(n.ty);
        let value = match &n.kind {
            NodeKind::Uniform { .. } => {
                let member = members.member(uniform_index).to_string();
                uniform_index += 1;
                let block = Expr::name(UNIFORMS, Ty::Struct(UNIFORM_STRUCT.into()));
                // Booleans are stored as u32.
                if ty == Ty::BOOL {
                    bin(BinaryOp::Ne, block.member(member, Ty::U32), Expr::u32(0))
                } else {
                    block.member(member, ty.clone())
                }
            }
            NodeKind::Context(read) => {
                let call = Expr::call(context_function(read, reflection), Vec::new(), Ty::of(read.ty()));
                cast(call, &ty)
            }
            NodeKind::Sample { texture, uv, default, grad } => {
                let t = reflection.textures.iter().find(|t| t.index == *texture).expect("live texture is reflected");
                let work = work_type(&ty);
                let fallback = cast(literal(default), &work);
                let uv = node(*uv, Ty::of(dag.node(*uv).ty));
                let grad = grad.map(|[dx, dy]| (node(dx, Ty::of(dag.node(dx).ty)), node(dy, Ty::of(dag.node(dy).ty))));
                cast(sample(t, uv, &work, fallback, grad, options), &ty)
            }
            NodeKind::Op { op: Op::Dpdx | Op::Dpdy, .. } if !options.implicit_derivatives => {
                literal(&Value::zero(n.ty))
            }
            NodeKind::Const(_) | NodeKind::Op { .. } => pure(dag, id),
        };
        lines.push(Stmt::Let { name: format!("n{i}"), ty, value: Some(value), mutable: false });
    }
    lines
}

/// Geomprops call the function reflection gave them (sanitised and unique); every other read calls
/// `ContextRead::function_name`.
pub fn context_function(read: &ContextRead, reflection: &Reflection) -> String {
    match read {
        ContextRead::GeomProp { name, .. } => {
            let g = reflection.context.geomprops.iter().find(|g| g.name == *name).expect("live geomprop is reflected");
            g.function.clone()
        }
        other => other.function_name(),
    }
}

/// The expression of a node that needs no host data: a constant or an op.
///
/// # Panics
/// On uniforms, context reads and samples.
pub fn pure(dag: &Dag, id: NodeId) -> Expr {
    let n = dag.node(id);
    let ty = Ty::of(n.ty);
    match &n.kind {
        NodeKind::Const(value) => cast(literal(value), &ty),
        NodeKind::Op { op: o, args } => {
            let args: Vec<Expr> = args.iter().map(|a| node(*a, Ty::of(dag.node(*a).ty))).collect();
            op(o, &args, &ty)
        }
        other => panic!("{other:?} reads host data"),
    }
}

/// A texture sample with MaterialX conventions: V flipped for top-row-first uploads, `constant` address mode as
/// an explicit bounds test returning `default`. A `select` evaluates both sides, so the sample stays in uniform
/// control flow. `grad` (MaterialX uv space, like `uv`) samples with explicit gradients, the flip negating their
/// v; without implicit derivatives a plain sample reads level 0.
fn sample(
    t: &TextureReflection,
    uv: Expr,
    ty: &Ty,
    default: Expr,
    grad: Option<(Expr, Expr)>,
    options: &MaterialOptions,
) -> Expr {
    let flip = |d: Expr| {
        if options.flip_v {
            Expr::construct(Ty::vec(2), vec![d.clone().swizzle("x"), Expr::unary(UnaryOp::Neg, d.swizzle("y"))])
        } else {
            d
        }
    };
    let v = if options.flip_v { sub(Expr::f32(1.0), uv.clone().swizzle("y")) } else { uv.clone().swizzle("y") };
    let st = Expr::construct(Ty::vec(2), vec![uv.clone().swizzle("x"), v]);
    let texture = Expr::name(texture_name(t.index), Ty::Texture2d);
    let sampler = Expr::name(sampler_name(t.index), Ty::Sampler);
    let fetch = match grad {
        None if options.implicit_derivatives => builtin(Builtin::TextureSample, vec![texture, sampler, st]),
        None => builtin(Builtin::TextureSampleLevel, vec![texture, sampler, st, Expr::f32(0.0)]),
        Some((dx, dy)) => builtin(Builtin::TextureSampleGrad, vec![texture, sampler, st, flip(dx), flip(dy)]),
    };
    let fetch = match ty.width() {
        Some(1) => fetch.swizzle("x"),
        Some(2) => fetch.swizzle("xy"),
        Some(3) => fetch.swizzle("xyz"),
        _ => fetch,
    };
    let outside = |c: &str| {
        let x = uv.clone().swizzle(c);
        bin(BinaryOp::Or, bin(BinaryOp::Lt, x.clone(), Expr::f32(0.0)), bin(BinaryOp::Gt, x, Expr::f32(1.0)))
    };
    let mut tests = Vec::new();
    if t.address[0] == AddressMode::Constant {
        tests.push(outside("x"));
    }
    if t.address[1] == AddressMode::Constant {
        tests.push(outside("y"));
    }
    match tests.into_iter().reduce(|a, b| bin(BinaryOp::Or, a, b)) {
        Some(cond) => select(fetch, default, cond),
        None => fetch,
    }
}
