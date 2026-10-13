//! One expression per IR op. Each family mirrors `crate::ops::<family>` and must compute what the interpreter
//! computes.

use super::values::*;
use crate::shader::ast::*;
use crate::{HOST_PREFIX, Op, TransformKind, Value, transform_name};

/// The expression of `op` over its arguments, of the node type `ty`.
pub fn op(op: &Op, args: &[Expr], ty: &Ty) -> Expr {
    match op {
        Op::Add
        | Op::Subtract
        | Op::Multiply
        | Op::Divide
        | Op::Modulo
        | Op::Invert
        | Op::Absval
        | Op::Sign
        | Op::Floor
        | Op::Ceil
        | Op::Round
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
        | Op::Remap => math(op, args, ty),
        Op::Sin | Op::Cos | Op::Tan | Op::Asin | Op::Acos | Op::Atan2 => trig(op, args, ty),
        Op::Normalize | Op::Magnitude | Op::Distance | Op::DotProduct | Op::CrossProduct | Op::Rotate2d => {
            vector(op, args, ty)
        }
        Op::Transform { kind, from, to } => {
            let v = cast(args[0].clone(), &Ty::vec(3));
            let host = format!("{HOST_PREFIX}{}", transform_name(*kind, *from, *to));
            let call = Expr::call(host, vec![v], Ty::vec(3));
            // Normals are renormalized, like the interpreter.
            let moved = if *kind == TransformKind::Normal { builtin(Builtin::Normalize, vec![call]) } else { call };
            cast(moved, ty)
        }
        Op::Greater
        | Op::GreaterEq
        | Op::Equal
        | Op::IfGreater
        | Op::IfGreaterEq
        | Op::IfEqual
        | Op::Switch
        | Op::And
        | Op::Or
        | Op::Not => logic(op, args, ty),
        Op::Extract(_) | Op::Combine | Op::Convert => channel(op, args, ty),
        Op::Luminance | Op::RgbToHsv | Op::HsvToRgb => color(op, args, ty),
        Op::Noise2d
        | Op::Noise3d
        | Op::Fractal3d
        | Op::CellNoise2d
        | Op::CellNoise3d
        | Op::WorleyNoise2d
        | Op::WorleyNoise3d => noise(op, args, ty),
        Op::NormalMap => {
            // in, scale, normal, tangent, bitangent; a scalar scale splats (mx_normalmap_float).
            let scale = cast(args[1].clone(), &Ty::vec(2));
            let v: Vec<Expr> = [0, 2, 3, 4].iter().map(|&i| leading(&args[i], 3)).collect();
            let call =
                lib_call("shadewire_normalmap", vec![v[0].clone(), scale, v[1].clone(), v[2].clone(), v[3].clone()]);
            cast(call, ty)
        }
        Op::Dpdx | Op::Dpdy => {
            let b = if *op == Op::Dpdx { Builtin::Dpdx } else { Builtin::Dpdy };
            let a = wide(args, ty);
            cast(builtin(b, vec![a[0].clone()]), ty)
        }
        Op::HextileCoord(_) | Op::HextileGrad(_) | Op::HextileWeights => hextile(op, args, ty),
    }
}

fn lit(x: f32) -> Expr {
    Expr::f32(x)
}

fn math(op: &Op, args: &[Expr], ty: &Ty) -> Expr {
    let a = wide(args, ty);
    let a0 = || a[0].clone();
    let a1 = || a[1].clone();
    let call = |b: Builtin, n: usize| builtin(b, a[..n].to_vec());
    let expr = match op {
        Op::Add => add(a0(), a1()),
        Op::Subtract => sub(a0(), a1()),
        Op::Multiply => mul(a0(), a1()),
        Op::Divide => div(a0(), a1()),
        Op::Modulo => sub(a0(), mul(a1(), builtin(Builtin::Floor, vec![div(a0(), a1())]))),
        Op::Invert => sub(a1(), a0()),
        Op::Absval => call(Builtin::Abs, 1),
        Op::Sign => call(Builtin::Sign, 1),
        Op::Floor => call(Builtin::Floor, 1),
        Op::Ceil => call(Builtin::Ceil, 1),
        Op::Round => call(Builtin::Round, 1),
        Op::Fract => sub(a0(), builtin(Builtin::Floor, vec![a0()])),
        Op::Power => call(Builtin::Pow, 2),
        Op::SafePower => mul(
            builtin(Builtin::Sign, vec![a0()]),
            builtin(Builtin::Pow, vec![builtin(Builtin::Abs, vec![a0()]), a1()]),
        ),
        Op::Sqrt => call(Builtin::Sqrt, 1),
        Op::Exp => call(Builtin::Exp, 1),
        Op::Ln => call(Builtin::Log, 1),
        Op::Min => call(Builtin::Min, 2),
        Op::Max => call(Builtin::Max, 2),
        Op::Clamp => builtin(Builtin::Min, vec![builtin(Builtin::Max, vec![a0(), a1()]), a[2].clone()]),
        Op::Mix => {
            let amount = if args[2].ty.is_scalar() { cast(args[2].clone(), &Ty::F32) } else { a[2].clone() };
            add(a1(), mul(sub(a0(), a1()), amount))
        }
        Op::Smoothstep => {
            let work = work_type(ty);
            let zero = cast(lit(0.0), &work);
            let one = cast(lit(1.0), &work);
            let t = div(sub(a0(), a1()), sub(a[2].clone(), a1()));
            let inner = mul(mul(t.clone(), t.clone()), sub(lit(3.0), mul(lit(2.0), t)));
            let low = select(inner, zero, bin(BinaryOp::Le, a0(), a1()));
            select(low, one, bin(BinaryOp::Ge, a0(), a[2].clone()))
        }
        Op::Remap => {
            add(a[3].clone(), div(mul(sub(a0(), a1()), sub(a[4].clone(), a[3].clone())), sub(a[2].clone(), a1())))
        }
        other => panic!("{other:?} is not a math op"),
    };
    cast(expr, ty)
}

fn trig(op: &Op, args: &[Expr], ty: &Ty) -> Expr {
    let a = wide(args, ty);
    let b = match op {
        Op::Sin => Builtin::Sin,
        Op::Cos => Builtin::Cos,
        Op::Tan => Builtin::Tan,
        Op::Asin => Builtin::Asin,
        Op::Acos => Builtin::Acos,
        // Arguments are iny, inx.
        Op::Atan2 => Builtin::Atan2,
        other => panic!("{other:?} is not a trig op"),
    };
    let n = if b == Builtin::Atan2 { 2 } else { 1 };
    cast(builtin(b, a[..n].to_vec()), ty)
}

/// Operands keep their own width (the first argument's); there is no scalar `normalize` or `dot`, so those are
/// written out.
fn vector(op: &Op, args: &[Expr], ty: &Ty) -> Expr {
    if *op == Op::Rotate2d {
        let call = lib_call("shadewire_rotate_vector2", vec![leading(&args[0], 2), first_f32(&args[1])]);
        return cast(call, ty);
    }
    if args.len() == 2 && args[0].ty.width() != args[1].ty.width() {
        panic!("{op:?} of different widths: {:?} and {:?}", args[0].ty, args[1].ty);
    }
    let operand = work_type(&args[0].ty);
    let a: Vec<Expr> = args.iter().map(|e| cast(e.clone(), &operand)).collect();
    let scalar = operand.is_scalar();
    let expr = match op {
        Op::Normalize if scalar => div(a[0].clone(), builtin(Builtin::Length, vec![a[0].clone()])),
        Op::Normalize => builtin(Builtin::Normalize, vec![a[0].clone()]),
        Op::Magnitude => builtin(Builtin::Length, vec![a[0].clone()]),
        Op::Distance => builtin(Builtin::Distance, a[..2].to_vec()),
        Op::DotProduct if scalar => mul(a[0].clone(), a[1].clone()),
        Op::DotProduct => builtin(Builtin::Dot, a[..2].to_vec()),
        Op::CrossProduct => builtin(Builtin::Cross, a[..2].to_vec()),
        other => panic!("{other:?} is not a vector op"),
    };
    cast(cast(expr, &work_type(ty)), ty)
}

/// Comparisons read `as_f32` (the first component, as f32); equality compares every component.
fn logic(op: &Op, args: &[Expr], ty: &Ty) -> Expr {
    let pick = |cond: Expr| select(cast(args[3].clone(), ty), cast(args[2].clone(), ty), cond);
    let greater = |op: BinaryOp| bin(op, first_f32(&args[0]), first_f32(&args[1]));
    let boolean = match op {
        Op::Greater => greater(BinaryOp::Gt),
        Op::GreaterEq => greater(BinaryOp::Ge),
        Op::Equal => equal(&args[0], &args[1]),
        Op::IfGreater => return pick(greater(BinaryOp::Gt)),
        Op::IfGreaterEq => return pick(greater(BinaryOp::Ge)),
        Op::IfEqual => return pick(equal(&args[0], &args[1])),
        Op::Switch => return switch(args, ty),
        Op::And => bin(BinaryOp::And, truth(&args[0]), truth(&args[1])),
        Op::Or => bin(BinaryOp::Or, truth(&args[0]), truth(&args[1])),
        Op::Not => Expr::unary(UnaryOp::Not, truth(&args[0])),
        other => panic!("{other:?} is not a logic op"),
    };
    cast(boolean, ty)
}

/// `floats() == floats()`: scalars as f32, vectors componentwise, different widths never equal.
fn equal(a: &Expr, b: &Expr) -> Expr {
    if a.ty.is_scalar() && b.ty.is_scalar() {
        bin(BinaryOp::Eq, first_f32(a), first_f32(b))
    } else if a.ty.width() == b.ty.width() {
        builtin(Builtin::All, vec![bin(BinaryOp::Eq, a.clone(), b.clone())])
    } else {
        Expr::bool(false)
    }
}

/// `Value::as_bool`.
fn truth(arg: &Expr) -> Expr {
    if arg.ty == Ty::BOOL { arg.clone() } else { bin(BinaryOp::Ne, first_f32(arg), lit(0.0)) }
}

/// NG_switch: in_k for the first k in 1..=10 with `which < k`, else zero (NaN included).
fn switch(args: &[Expr], ty: &Ty) -> Expr {
    let which = first_f32(&args[10]);
    let mut out = literal(&Value::zero(ir_type(ty)));
    for k in (1..=10).rev() {
        let input = cast(args[k - 1].clone(), ty);
        out = select(out, input, bin(BinaryOp::Lt, which.clone(), lit(k as f32)));
    }
    out
}

/// The IR type of a node's shader type (colors read as vectors: zero is the same).
fn ir_type(ty: &Ty) -> crate::Type {
    match ty {
        Ty::Scalar(Scalar::Bool) => crate::Type::Boolean,
        Ty::Scalar(Scalar::I32) => crate::Type::Integer,
        Ty::Vector(2, _) => crate::Type::Vector2,
        Ty::Vector(3, _) => crate::Type::Vector3,
        Ty::Vector(4, _) => crate::Type::Vector4,
        _ => crate::Type::Float,
    }
}

fn channel(op: &Op, args: &[Expr], ty: &Ty) -> Expr {
    match op {
        Op::Extract(index) => {
            let from = &args[0];
            let width = from.ty.width().unwrap_or(0);
            assert!(
                *index < width as u32,
                "extract index {index} of a {} {:?}",
                if from.ty.is_scalar() { "scalar" } else { "narrower" },
                from.ty
            );
            let component = if from.ty.is_scalar() {
                cast(from.clone(), &Ty::F32)
            } else {
                from.clone().index(Expr::i32(*index as i32))
            };
            cast(component, ty)
        }
        Op::Combine => {
            let parts =
                args.iter().map(|a| if a.ty.is_scalar() { cast(a.clone(), &Ty::F32) } else { a.clone() }).collect();
            Expr::construct(ty.clone(), parts)
        }
        // The ND_convert_* graphs: the same width relabels, a scalar splats to every channel (alpha included),
        // wider to narrower drops trailing channels, narrower to wider appends 1.0.
        Op::Convert => {
            let arg = &args[0];
            let have = arg.ty.width().unwrap_or(1);
            let want = ty.width().unwrap_or(1);
            if have == 1 || have == want {
                return cast(arg.clone(), ty);
            }
            if have > want {
                let narrowed = arg.clone().swizzle(&"xyzw"[..want as usize]);
                // One component is an f32 that may still become an integer or a boolean.
                return if want == 1 { cast(narrowed, ty) } else { narrowed };
            }
            let mut parts = vec![arg.clone()];
            parts.extend((have..want).map(|_| lit(1.0)));
            Expr::construct(ty.clone(), parts)
        }
        other => panic!("{other:?} is not a channel op"),
    }
}

/// The first three channels go through the function, color4 keeps its alpha. Luminance is written out in the
/// interpreter's order, not `dot`.
fn color(op: &Op, args: &[Expr], ty: &Ty) -> Expr {
    let rgb = match op {
        Op::Luminance => {
            let (c, k) = (&args[0], &args[1]);
            assert!(k.ty.width().unwrap_or(0) >= 3, "luminance coefficients of type {:?}", k.ty);
            assert!(c.ty.width().unwrap_or(0) >= 3, "luminance of type {:?}", c.ty);
            let at = |e: &Expr, i: i32| e.clone().index(Expr::i32(i));
            let sum = add(add(mul(at(c, 0), at(k, 0)), mul(at(c, 1), at(k, 1))), mul(at(c, 2), at(k, 2)));
            Expr::construct(Ty::vec(3), vec![sum])
        }
        Op::RgbToHsv => lib_call("shadewire_rgbtohsv", vec![leading(&args[0], 3)]),
        Op::HsvToRgb => lib_call("shadewire_hsvtorgb", vec![leading(&args[0], 3)]),
        other => panic!("{other:?} is not a color op"),
    };
    if *ty == Ty::vec(4) && args[0].ty == Ty::vec(4) {
        Expr::construct(Ty::vec(4), vec![rgb, args[0].clone().index(Expr::i32(3))])
    } else {
        assert!(*ty != Ty::vec(4), "{op:?} to color4 needs a four-channel input, got {:?}", args[0].ty);
        cast(rgb, ty)
    }
}

/// The genglsl noise formulas over the support library; arguments in `catalog::input_names` order.
fn noise(op: &Op, args: &[Expr], ty: &Ty) -> Expr {
    let width = ty.width().unwrap_or(1);
    match op {
        Op::Noise2d | Op::Noise3d => {
            let (n, dim, offset) = if *op == Op::Noise2d {
                (2, "2d", Expr::construct(Ty::vec(2), vec![lit(19.0), lit(73.0)]))
            } else {
                (3, "3d", Expr::construct(Ty::vec(3), vec![lit(19.0), lit(73.0), lit(29.0)]))
            };
            let p = leading(&args[2], n);
            let float = format!("shadewire_perlin_noise_float_{dim}");
            let vec3 = format!("shadewire_perlin_noise_vec3_{dim}");
            let value = match width {
                1 => lib_call(&float, vec![p]),
                2 => lib_call(&vec3, vec![p]).swizzle("xy"),
                3 => lib_call(&vec3, vec![p]),
                _ => Expr::construct(
                    Ty::vec(4),
                    vec![lib_call(&vec3, vec![p.clone()]), lib_call(&float, vec![add(p, offset)])],
                ),
            };
            scale(value, &args[0], Some(first_f32(&args[1])), ty)
        }
        Op::Fractal3d => {
            let p = leading(&args[4], 3);
            let suffix = ["float", "vec2", "vec3", "vec4"][width as usize - 1];
            let call = lib_call(
                &format!("shadewire_fractal_noise_{suffix}_3d"),
                vec![p, int(&args[1]), first_f32(&args[2]), first_f32(&args[3])],
            );
            scale(call, &args[0], None, ty)
        }
        Op::CellNoise2d => cast(lib_call("shadewire_cell_noise_float_2d", vec![leading(&args[0], 2)]), ty),
        Op::CellNoise3d => cast(lib_call("shadewire_cell_noise_float_3d", vec![leading(&args[0], 3)]), ty),
        Op::WorleyNoise2d | Op::WorleyNoise3d => {
            let (p, dim) =
                if *op == Op::WorleyNoise2d { (leading(&args[0], 2), "2d") } else { (leading(&args[0], 3), "3d") };
            // Metric 0 (Euclidean); any width above two reads the vec3 variant, as the interpreter's `_` arm does.
            let suffix = match width {
                1 => "float",
                2 => "vec2",
                _ => "vec3",
            };
            let call = lib_call(
                &format!("shadewire_worley_noise_{suffix}_{dim}"),
                vec![p, first_f32(&args[1]), int(&args[2]), Expr::i32(0)],
            );
            cast(call, ty)
        }
        other => panic!("{other:?} is not a noise op"),
    }
}

/// `value * amplitude + pivot`, amplitude broadcast to the result type. The fractal has no pivot: the
/// interpreter's `+ 0.0` only turns -0.0 into +0.0, which compare equal.
fn scale(value: Expr, amplitude: &Expr, pivot: Option<Expr>, ty: &Ty) -> Expr {
    let work = work_type(ty);
    let scaled = mul(value, cast(amplitude.clone(), &work));
    let expr = match pivot {
        Some(p) => add(scaled, cast(p, &work)),
        None => scaled,
    };
    cast(expr, ty)
}

/// Integer and enum inputs (octaves, style): an i32 as is, anything else as `i32(first component)`, like the
/// interpreter's `as i32` (both round toward zero).
fn int(arg: &Expr) -> Expr {
    if arg.ty == Ty::I32 { arg.clone() } else { first_f32(arg).convert(Ty::I32) }
}

/// Calls into the hextile library, arguments in `catalog::input_names` order, each cast to the type the library
/// function takes; the tile index follows the coordinate.
fn hextile(op: &Op, args: &[Expr], ty: &Ty) -> Expr {
    let v2 = Ty::vec(2);
    let (function, tile, types): (&str, Option<u8>, Vec<Ty>) = match op {
        Op::HextileCoord(tile) => (
            "shadewire_hextile_coord",
            Some(*tile),
            vec![v2.clone(), Ty::F32, v2.clone(), Ty::F32, v2.clone(), Ty::F32, v2],
        ),
        Op::HextileGrad(tile) => {
            ("shadewire_hextile_grad", Some(*tile), vec![v2.clone(), v2.clone(), Ty::F32, v2.clone(), Ty::F32, v2])
        }
        Op::HextileWeights => ("shadewire_hextile_weights", None, vec![v2, Ty::vec(3), Ty::F32]),
        other => panic!("{other:?} is not a hextile op"),
    };
    let mut a: Vec<Expr> = args.iter().zip(&types).map(|(e, to)| cast(e.clone(), to)).collect();
    if let Some(tile) = tile {
        let at = if matches!(op, Op::HextileGrad(_)) { 2 } else { 1 };
        a.insert(at, Expr::i32(tile as i32));
    }
    cast(lib_call(function, a), ty)
}
