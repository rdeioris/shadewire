//! What the shared material tree needs to become Godot code. Godot has no uniform blocks and no stage built-ins
//! outside `vertex()`/`fragment()`/`light()`, so:
//!
//! - reads of the uniform block (`shadewire_u.m`, booleans as `shadewire_u.m != 0u`) read the named global;
//! - host function calls (`shadewire_host_texcoord_0()`) read the context `fragment()` fills (`ctx.texcoord_0`),
//!   and transforms (`shadewire_host_transform_point_object_to_world(v)`) multiply by its matrices;
//! - the uniform struct, the resources and the host declarations go, and `shadewire_eval_surface` takes the
//!   context.

use std::collections::HashMap;

use shadewire::shader::*;
use shadewire::{HOST_PREFIX, Reflection};

use crate::context::{CONTEXT, CONTEXT_STRUCT};

const UNIFORMS: &str = "shadewire_u";

/// Applies `f` to every expression of the module's functions and constants, innermost first.
fn map_exprs(module: &mut Module, f: &mut impl FnMut(&mut Expr)) {
    fn expr(e: &mut Expr, f: &mut impl FnMut(&mut Expr)) {
        match &mut e.kind {
            ExprKind::Call { args, .. } | ExprKind::Builtin { args, .. } | ExprKind::Construct(args) => {
                args.iter_mut().for_each(|a| expr(a, f))
            }
            ExprKind::Convert(a)
            | ExprKind::Bitcast(a)
            | ExprKind::Deref(a)
            | ExprKind::AddressOf(a)
            | ExprKind::Unary { arg: a, .. }
            | ExprKind::Member { base: a, .. }
            | ExprKind::Swizzle { base: a, .. } => expr(a, f),
            ExprKind::Binary { lhs, rhs, .. } => {
                expr(lhs, f);
                expr(rhs, f);
            }
            ExprKind::Index { base, index } => {
                expr(base, f);
                expr(index, f);
            }
            ExprKind::Literal(_) | ExprKind::Name(_) => {}
        }
        f(e);
    }
    fn stmts(body: &mut [Stmt], f: &mut impl FnMut(&mut Expr)) {
        for s in body {
            match s {
                Stmt::Let { value: Some(v), .. } | Stmt::Return(Some(v)) | Stmt::Expr(v) => expr(v, f),
                Stmt::Assign { target, value, .. } => {
                    expr(target, f);
                    expr(value, f);
                }
                Stmt::If { cond, then, otherwise } => {
                    expr(cond, f);
                    stmts(then, f);
                    stmts(otherwise, f);
                }
                Stmt::For { init, cond, update, body } => {
                    stmts(std::slice::from_mut(init), f);
                    expr(cond, f);
                    stmts(std::slice::from_mut(update), f);
                    stmts(body, f);
                }
                Stmt::Switch { selector, cases, default } => {
                    expr(selector, f);
                    for (values, body) in cases {
                        values.iter_mut().for_each(|v| expr(v, f));
                        stmts(body, f);
                    }
                    stmts(default, f);
                }
                _ => {}
            }
        }
    }
    for item in &mut module.items {
        match item {
            Item::Const { value, .. } => expr(value, f),
            Item::Function(func) => stmts(&mut func.body, f),
            _ => {}
        }
    }
}

/// `shadewire_u.member`'s member name, when `e` is one.
fn block_member(e: &Expr) -> Option<&str> {
    match &e.kind {
        ExprKind::Member { base, member } if matches!(&base.kind, ExprKind::Name(n) if n == UNIFORMS) => Some(member),
        _ => None,
    }
}

/// `transform_<kind>_<from>_to_<to>` as its parts.
fn transform_parts(name: &str) -> Option<(&str, &str, &str)> {
    let rest = name.strip_prefix("transform_")?;
    let (kind, spaces) = rest.split_once('_')?;
    let (from, to) = spaces.split_once("_to_")?;
    Some((kind, from, to))
}

/// A space change through the context's matrices. Godot has one object space, so model and object are the same
/// and a change between them is the identity.
fn transform(kind: &str, from: &str, to: &str, v: Expr) -> Expr {
    let m4 = || Expr::name(CONTEXT, Ty::Struct(CONTEXT_STRUCT.into())).member("model_matrix", Ty::Matrix(4, 4));
    let normal =
        || Expr::name(CONTEXT, Ty::Struct(CONTEXT_STRUCT.into())).member("model_normal_matrix", Ty::Matrix(3, 3));
    let mat3 = |m: Expr| Expr::construct(Ty::Matrix(3, 3), vec![m]);
    let inverse = |m: Expr| {
        let ty = m.ty.clone();
        Expr::call("inverse", vec![m], ty)
    };
    let times = |m: Expr, x: Expr, ty: Ty| Expr::binary(BinaryOp::Mul, m, x, ty);
    let point =
        |m: Expr, v: Expr| times(m, Expr::construct(Ty::vec(4), vec![v, Expr::f32(1.0)]), Ty::vec(4)).swizzle("xyz");
    match (from == "world", to == "world") {
        (false, true) => match kind {
            "point" => point(m4(), v),
            "vector" => times(mat3(m4()), v, Ty::vec(3)),
            _ => times(normal(), v, Ty::vec(3)),
        },
        (true, false) => match kind {
            "point" => point(inverse(m4()), v),
            "vector" => times(inverse(mat3(m4())), v, Ty::vec(3)),
            // A normal's inverse transpose, inverted: the transpose of the model matrix's upper 3x3.
            _ => times(Expr::builtin(Builtin::Transpose, vec![mat3(m4())], Ty::Matrix(3, 3)), v, Ty::vec(3)),
        },
        _ => v,
    }
}

/// Whether the module's transforms need the model matrices in the context.
pub fn needs_matrices(reflection: &Reflection) -> bool {
    reflection
        .context
        .transforms
        .iter()
        .filter_map(|t| transform_parts(t))
        .any(|(_, f, t)| (f == "world") != (t == "world"))
}

/// Rewrites the module for Godot. `uniform_names[i]` is the Godot name of reflection uniform `i`; `members` maps a
/// host function's name to its context member; `has_context` says whether `fragment()` passes a context.
pub fn for_godot(
    module: &mut Module,
    reflection: &Reflection,
    uniform_names: &[String],
    members: &HashMap<String, String>,
    has_context: bool,
) {
    let block = shadewire::shader::names::UniformNames::new(reflection);
    let by_member: HashMap<String, String> =
        (0..reflection.uniforms.len()).map(|i| (block.member(i).to_string(), uniform_names[i].clone())).collect();
    module.items.retain(|item| match item {
        Item::Struct(s) => s.layout.is_none(),
        Item::Resource(_) | Item::Extern(_) => false,
        _ => true,
    });
    let booleans: Vec<&String> = reflection
        .uniforms
        .iter()
        .zip(uniform_names)
        .filter(|(u, _)| u.ty == shadewire::Type::Boolean)
        .map(|(_, n)| n)
        .collect();
    map_exprs(module, &mut |e| {
        // A boolean uniform: `shadewire_u.m != 0u` (its member already renamed, inner expressions go first)
        // reads Godot's native `bool`.
        if let ExprKind::Binary { op: BinaryOp::Ne, lhs, rhs } = &e.kind
            && matches!(rhs.kind, ExprKind::Literal(Literal::U32(0)))
            && let ExprKind::Name(n) = &lhs.kind
            && booleans.contains(&n)
        {
            *e = Expr::name(n.clone(), Ty::BOOL);
            return;
        }
        if let Some(m) = block_member(e) {
            *e = Expr::name(by_member[m].clone(), e.ty.clone());
            return;
        }
        if let ExprKind::Call { function, args } = &e.kind
            && let Some(host) = function.strip_prefix(HOST_PREFIX)
        {
            if let Some((kind, from, to)) = transform_parts(host)
                && args.len() == 1
            {
                *e = transform(kind, from, to, args[0].clone());
            } else if let Some(member) = members.get(function.as_str()) {
                *e = Expr::name(CONTEXT, Ty::Struct(CONTEXT_STRUCT.into())).member(member.clone(), e.ty.clone());
            }
        }
    });
    if has_context {
        for item in &mut module.items {
            if let Item::Function(f) = item
                && f.name == "shadewire_eval_surface"
            {
                f.params.push((CONTEXT.into(), Ty::Struct(CONTEXT_STRUCT.into())));
            }
        }
    }
}

/// Every name the module declares (functions, parameters, locals, structs, constants), which no Godot global may
/// take: Godot refuses a parameter or local named like a global.
pub fn declared_names(module: &Module) -> Vec<String> {
    fn stmts(body: &[Stmt], out: &mut Vec<String>) {
        for s in body {
            match s {
                Stmt::Let { name, .. } => out.push(name.clone()),
                Stmt::If { then, otherwise, .. } => {
                    stmts(then, out);
                    stmts(otherwise, out);
                }
                Stmt::For { init, update, body, .. } => {
                    stmts(std::slice::from_ref(init), out);
                    stmts(std::slice::from_ref(update), out);
                    stmts(body, out);
                }
                Stmt::Switch { cases, default, .. } => {
                    cases.iter().for_each(|(_, b)| stmts(b, out));
                    stmts(default, out);
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for item in &module.items {
        match item {
            Item::Function(f) | Item::Extern(f) => {
                out.push(f.name.clone());
                out.extend(f.params.iter().map(|(n, _)| n.clone()));
                stmts(&f.body, &mut out);
            }
            Item::Struct(s) => out.push(s.name.clone()),
            Item::Const { name, .. } => out.push(name.clone()),
            _ => {}
        }
    }
    out
}
