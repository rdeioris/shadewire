//! The shared shader tree as MDL: C-like statements, `math::` built-ins, no swizzles of more than one component,
//! no unsigned integers (`u32` is `int`, its shifts right `>>>` and its order and conversions done unsigned), no
//! pointers, no samplers.

use shadewire::shader::print::{Printer, Syntax};
use shadewire::shader::*;

/// The MDL syntax.
pub struct Mdl;

const COMPONENTS: [&str; 4] = ["x", "y", "z", "w"];

/// The `i`th component of `e`, a vector, or `e` itself when it is a scalar.
fn component(p: &Printer<Mdl>, e: &Expr, i: usize) -> String {
    if e.ty.is_vector() { format!("{}.{}", p.operand(e, 12), COMPONENTS[i]) } else { p.expr(e) }
}

fn is_u32(e: &Expr) -> bool {
    e.ty.scalar() == Some(Scalar::U32)
}

/// Zero of `ty`.
fn zero(ty: &Ty) -> String {
    match ty.width() {
        Some(1) | None => "0.0".to_string(),
        Some(n) => format!("float{n}(0.0)"),
    }
}

impl Syntax for Mdl {
    fn ty(&self, ty: &Ty) -> String {
        let scalar = |s: Scalar| match s {
            Scalar::Bool => "bool",
            Scalar::I32 | Scalar::U32 => "int",
            Scalar::F32 => "float",
        };
        match ty {
            Ty::Scalar(s) => scalar(*s).into(),
            Ty::Vector(n, s) => format!("{}{n}", scalar(*s)),
            Ty::Matrix(c, r) => format!("float{c}x{r}"),
            Ty::Struct(name) => name.clone(),
            Ty::Texture2d => "texture_2d".into(),
            Ty::Void => "void".into(),
            Ty::Ptr(_) | Ty::Sampler | Ty::Unknown => panic!("MDL has no {ty:?}"),
        }
    }

    fn builtin_name(&self, b: Builtin) -> &'static str {
        match b {
            Builtin::Fract => "math::frac",
            Builtin::InverseSqrt => "math::rsqrt",
            Builtin::Mix => "math::lerp",
            Builtin::Abs => "math::abs",
            Builtin::Sign => "math::sign",
            Builtin::Floor => "math::floor",
            Builtin::Ceil => "math::ceil",
            Builtin::Round => "math::round",
            Builtin::Pow => "math::pow",
            Builtin::Sqrt => "math::sqrt",
            Builtin::Exp => "math::exp",
            Builtin::Exp2 => "math::exp2",
            Builtin::Log => "math::log",
            Builtin::Log2 => "math::log2",
            Builtin::Min => "math::min",
            Builtin::Max => "math::max",
            Builtin::Clamp => "math::clamp",
            Builtin::Smoothstep => "math::smoothstep",
            Builtin::Step => "math::step",
            Builtin::Sin => "math::sin",
            Builtin::Cos => "math::cos",
            Builtin::Tan => "math::tan",
            Builtin::Asin => "math::asin",
            Builtin::Acos => "math::acos",
            Builtin::Atan => "math::atan",
            Builtin::Atan2 => "math::atan2",
            Builtin::Radians => "math::radians",
            Builtin::Degrees => "math::degrees",
            Builtin::Normalize => "math::normalize",
            Builtin::Length => "math::length",
            Builtin::Distance => "math::distance",
            Builtin::Dot => "math::dot",
            Builtin::Cross => "math::cross",
            Builtin::Transpose => "math::transpose",
            Builtin::All => "math::all",
            Builtin::Any => "math::any",
            other => panic!("{other:?} is printed by Mdl::builtin"),
        }
    }

    fn literal(&self, lit: &Literal) -> String {
        match lit {
            Literal::Bool(b) => b.to_string(),
            Literal::I32(i) => i.to_string(),
            // The same bits, read as MDL's only integer type.
            Literal::U32(u) => (*u as i32).to_string(),
            Literal::F32(x) => shadewire::shader::print::float(*x),
        }
    }

    fn builtin(&self, p: &Printer<Self>, b: Builtin, args: &[Expr], ty: &Ty) -> String {
        match b {
            Builtin::Select => {
                let (f, t, c) = (&args[0], &args[1], &args[2]);
                match c.ty.width() {
                    Some(n) if n > 1 => {
                        let parts: Vec<String> = (0..n as usize)
                            .map(|i| {
                                format!("{} ? {} : {}", component(p, c, i), component(p, t, i), component(p, f, i))
                            })
                            .collect();
                        format!("{}({})", self.ty(ty), parts.join(", "))
                    }
                    _ => format!("({} ? {} : {})", p.expr(c), p.expr(t), p.expr(f)),
                }
            }
            // MDL has no screen-space derivatives.
            Builtin::Dpdx | Builtin::Dpdy => zero(ty),
            Builtin::Fma => {
                format!("({} * {} + {})", p.operand(&args[0], 10), p.operand(&args[1], 10), p.operand(&args[2], 9))
            }
            Builtin::Trunc => {
                let a = p.expr(&args[0]);
                format!("(math::sign({a}) * math::floor(math::abs({a})))")
            }
            Builtin::Reflect => {
                let (i, n) = (p.expr(&args[0]), p.expr(&args[1]));
                format!("({i} - 2.0 * math::dot({n}, {i}) * {n})")
            }
            Builtin::TextureSample | Builtin::TextureSampleLevel | Builtin::TextureSampleGrad => {
                panic!("texture samples are rewritten to tex::lookup_float4 before printing")
            }
            other => format!("{}({})", self.builtin_name(other), p.list(args)),
        }
    }

    /// Vectors take scalars: a vector argument of a constructor of more than one is spread into its components.
    fn construct(&self, p: &Printer<Self>, ty: &Ty, args: &[Expr]) -> String {
        match ty {
            Ty::Vector(..) if args.len() > 1 => {
                let mut parts = Vec::new();
                for a in args {
                    match a.ty.width() {
                        Some(n) if n > 1 => parts.extend((0..n as usize).map(|i| component(p, a, i))),
                        _ => parts.push(p.expr(a)),
                    }
                }
                format!("{}({})", self.ty(ty), parts.join(", "))
            }
            _ => format!("{}({})", self.ty(ty), p.list(args)),
        }
    }

    fn convert(&self, p: &Printer<Self>, ty: &Ty, arg: &Expr) -> String {
        if is_u32(arg) && ty.scalar() == Some(Scalar::F32) {
            // Unsigned: the top 31 bits doubled, then the lowest.
            let unsigned = |x: String| format!("(float({x} >>> 1) * 2.0 + float({x} & 1))");
            return match arg.ty.width() {
                Some(n) if n > 1 => {
                    let parts: Vec<String> = (0..n as usize).map(|i| unsigned(component(p, arg, i))).collect();
                    format!("{}({})", self.ty(ty), parts.join(", "))
                }
                _ => unsigned(p.operand(arg, 12)),
            };
        }
        format!("{}({})", self.ty(ty), p.expr(arg))
    }

    fn bitcast(&self, p: &Printer<Self>, ty: &Ty, arg: &Expr) -> String {
        match (arg.ty.scalar(), ty.scalar()) {
            (Some(Scalar::I32 | Scalar::U32), Some(Scalar::F32)) => format!("math::int_bits_to_float({})", p.expr(arg)),
            (Some(Scalar::F32), Some(Scalar::I32 | Scalar::U32)) => format!("math::float_bits_to_int({})", p.expr(arg)),
            _ => p.expr(arg),
        }
    }

    fn binary(&self, p: &Printer<Self>, op: BinaryOp, lhs: &Expr, rhs: &Expr, ty: &Ty) -> Option<String> {
        if op == BinaryOp::Shr && is_u32(lhs) {
            return Some(format!("{} >>> {}", p.operand(lhs, 9), p.operand(rhs, 9)));
        }
        if op.is_comparison() && lhs.ty.is_vector() {
            let n = lhs.ty.width().expect("a vector") as usize;
            let parts: Vec<String> =
                (0..n).map(|i| format!("{} {} {}", component(p, lhs, i), op.symbol(), component(p, rhs, i))).collect();
            return Some(format!("{}({})", self.ty(ty), parts.join(", ")));
        }
        if matches!(op, BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge) && is_u32(lhs) {
            // Unsigned order: flipping the sign bit maps it onto signed order.
            let flip = |e: &Expr| format!("({} ^ -2147483648)", p.operand(e, 5));
            return Some(format!("{} {} {}", flip(lhs), op.symbol(), flip(rhs)));
        }
        if op == BinaryOp::Rem && lhs.ty.scalar() == Some(Scalar::F32) {
            return Some(format!("math::fmod({}, {})", p.expr(lhs), p.expr(rhs)));
        }
        None
    }

    fn swizzle(&self, p: &Printer<Self>, base: &Expr, components: &str, ty: &Ty) -> String {
        let base_text = p.operand(base, 12);
        if components.len() == 1 {
            return format!("{base_text}.{components}");
        }
        let parts: Vec<String> = components.chars().map(|c| format!("{base_text}.{c}")).collect();
        format!("{}({})", self.ty(ty), parts.join(", "))
    }

    fn deref(&self, _p: &Printer<Self>, _inner: &Expr) -> String {
        panic!("MDL has no pointers")
    }

    fn address_of(&self, _p: &Printer<Self>, _inner: &Expr) -> String {
        panic!("MDL has no pointers")
    }

    fn param(&self, name: &str, ty: &Ty) -> String {
        format!("{} {name}", self.ty(ty))
    }

    fn extern_decl(&self, _f: &Function, _module: &Module) -> Option<String> {
        None
    }

    fn resource(&self, _r: &Resource, _module: &Module) -> String {
        String::new()
    }
}

/// Prints a module as MDL (its items; the `mdl` line, imports and material are the caller's).
pub fn print(module: &Module) -> String {
    Printer::new(&Mdl).module(module)
}
