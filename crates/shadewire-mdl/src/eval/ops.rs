//! Operators, conversions, constructors and selects over values: folded in Rust when known, IR ops otherwise.

use std::rc::Rc;

use shadewire::{NodeKind, Op, Type, Value};

use super::Eval;
use crate::ast::{BinOp, UnaryOp};
use crate::types::{Len, Scalar, TextureShape, Ty};
use crate::value::{Df, Gamma, Num, Texture, V, Val};

/// What went wrong in an operator, before a caller adds where.
pub type OpResult<T> = Result<T, String>;

fn kb(b: bool) -> Num {
    Num::Known(Value::Boolean(b))
}

fn ki(i: i32) -> Num {
    Num::Known(Value::Integer(i))
}

fn kf(x: f32) -> Num {
    Num::Known(Value::Float(x))
}

/// The scalar kind two operands combine to: floating if either is, else integer.
fn combined(a: Scalar, b: Scalar) -> Scalar {
    match (a, b) {
        (Scalar::Double, _) | (_, Scalar::Double) => Scalar::Double,
        (Scalar::Float, _) | (_, Scalar::Float) => Scalar::Float,
        _ => Scalar::Int,
    }
}

impl<'p, 'r, 'a> Eval<'p, 'r, 'a> {
    // ---- components

    /// The components of a numeric value, each a scalar of the value's component type (a color's are floats).
    pub fn parts(&mut self, v: &Val, origin: &str) -> OpResult<Vec<Num>> {
        match &v.v {
            V::Parts(p) => Ok(p.clone()),
            V::Num(n) => {
                let width = v.ty.width().ok_or_else(|| format!("{} has no components", self.type_name(&v.ty)))?;
                if width == 1 {
                    return Ok(vec![n.clone()]);
                }
                (0..width as u32).map(|i| self.extract(n, i, origin)).collect()
            }
            _ => Err(format!("{} has no components", self.type_name(&v.ty))),
        }
    }

    /// Component `i` of the vector `n`, as a float.
    pub fn extract(&mut self, n: &Num, i: u32, origin: &str) -> OpResult<Num> {
        if let Num::Known(v) = n {
            return Ok(kf(v.floats()[i as usize]));
        }
        // Through a combine, take the input instead of a new extract.
        if let Num::Node(id) = n
            && let NodeKind::Op { op: Op::Combine, args } = &self.e.dag.node(*id).kind
            && args.len() == self.num_type(n).component_count()
        {
            let arg = args[i as usize];
            return Ok(match &self.e.dag.node(arg).kind {
                NodeKind::Const(v) => Num::Known(v.clone()),
                _ => Num::Node(arg),
            });
        }
        self.emit(Op::Extract(i), std::slice::from_ref(n), Type::Float, origin).map_err(|e| e.to_string())
    }

    /// A value of `ty` from its components (scalars of its component type).
    pub fn from_parts(&mut self, ty: &Ty, parts: Vec<Num>, origin: &str) -> OpResult<Val> {
        match ty.ir() {
            Some(ir) if ir.is_scalar() => Ok(Val::new(ty.clone(), V::Num(parts.into_iter().next().expect("one")))),
            Some(ir) => {
                if parts.iter().all(|p| p.known().is_some()) {
                    let floats: Vec<f32> = parts.iter().map(|p| p.known().expect("known").as_f32()).collect();
                    return Ok(Val::known(ty.clone(), Value::from_floats(ir, &floats)));
                }
                let n = self.emit(Op::Combine, &parts, ir, origin).map_err(|e| e.to_string())?;
                Ok(Val::new(ty.clone(), V::Num(n)))
            }
            None => Ok(Val::new(ty.clone(), V::Parts(parts))),
        }
    }

    /// The IR op `op` over known-or-node numbers, with a message for errors.
    fn op(&mut self, op: Op, args: &[Num], ty: Type, origin: &str) -> OpResult<Num> {
        self.emit(op, args, ty, origin).map_err(|e| e.to_string())
    }

    // ---- scalar conversions

    /// The scalar `n` (of kind `from`) as kind `to`.
    pub fn convert_scalar(&mut self, n: &Num, from: Scalar, to: Scalar, origin: &str) -> OpResult<Num> {
        let ir = |s: Scalar| match s {
            Scalar::Bool => Type::Boolean,
            Scalar::Int => Type::Integer,
            _ => Type::Float,
        };
        if ir(from) == ir(to) {
            return Ok(n.clone());
        }
        if let Num::Known(v) = n {
            let x = v.as_f32();
            return Ok(match (v, to) {
                (Value::Integer(i), Scalar::Bool) => kb(*i != 0),
                (_, Scalar::Bool) => kb(x != 0.0),
                (Value::Boolean(b), Scalar::Int) => ki(*b as i32),
                // MDL truncates toward zero; out of range saturates, as Rust's `as` does.
                (_, Scalar::Int) => ki(x as i32),
                (Value::Integer(i), _) => kf(*i as f32),
                _ => kf(x),
            });
        }
        if to == Scalar::Bool {
            let zero = if from == Scalar::Int { ki(0) } else { kf(0.0) };
            let equal = self.op(Op::Equal, &[n.clone(), zero], Type::Boolean, origin)?;
            return self.op(Op::Not, &[equal], Type::Boolean, origin);
        }
        if ir(to) == Type::Integer && ir(from) == Type::Float {
            // Truncation toward zero, as an integer floor (MaterialX converts no float to an integer).
            let t = self.trunc(n, Type::Float, origin)?;
            return self.op(Op::Floor, &[t], Type::Integer, origin);
        }
        self.op(Op::Convert, std::slice::from_ref(n), ir(to), origin)
    }

    /// `v` as type `to`: the implicit conversions (MDL 6.9.2, 6.10.2, enums to integers, arrays to size-deferred
    /// arrays), and with `explicit` also the conversions a constructor makes (`float(i)`, `int(x)`, `float3(c)`).
    pub fn convert_val(&mut self, v: Val, to: &Ty, explicit: bool, origin: &str) -> OpResult<Val> {
        if &v.ty == to {
            return Ok(v);
        }
        let mismatch = |s: &Self| format!("cannot convert {} to {}", s.type_name(&v.ty), s.type_name(to));
        match (&v.ty, to) {
            (Ty::Enum(_), Ty::Scalar(s)) => {
                let i = v.known_int().ok_or_else(|| mismatch(self))?;
                let n = self.convert_scalar(&ki(i), Scalar::Int, *s, origin)?;
                Ok(Val::new(to.clone(), V::Num(n)))
            }
            (Ty::Scalar(from), Ty::Scalar(s)) if explicit || from.converts_to(*s) => {
                let n = v.num().expect("a scalar is a number").clone();
                let n = self.convert_scalar(&n, *from, *s, origin)?;
                Ok(Val::new(to.clone(), V::Num(n)))
            }
            (Ty::Vector(from, n), Ty::Vector(s, m)) if n == m && (explicit || from.converts_to(*s)) => {
                if from.is_floating() && s.is_floating() {
                    return Ok(Val::new(to.clone(), v.v));
                }
                let parts = self.parts(&v, origin)?;
                let from = *from;
                let parts =
                    parts.iter().map(|p| self.convert_scalar(p, from, *s, origin)).collect::<OpResult<Vec<_>>>()?;
                self.from_parts(to, parts, origin)
            }
            // Colors convert only explicitly, to and from three-component vectors.
            (Ty::Color, Ty::Vector(_, 3)) | (Ty::Vector(_, 3), Ty::Color) if explicit => {
                let parts = self.parts(&v, origin)?;
                let from = v.ty.scalar().expect("numeric");
                let parts = parts
                    .iter()
                    .map(|p| self.convert_scalar(p, from, to.scalar().expect("numeric"), origin))
                    .collect::<OpResult<Vec<_>>>()?;
                if let (Some(_), V::Num(n)) = (to.ir(), &v.v)
                    && from.is_floating()
                {
                    let ir = to.ir().expect("an IR type");
                    let n = self.op(Op::Convert, std::slice::from_ref(n), ir, origin)?;
                    return Ok(Val::new(to.clone(), V::Num(n)));
                }
                self.from_parts(to, parts, origin)
            }
            // What the MDL SDK's compiler converts implicitly beyond the specification's rules: a float to an int
            // where an int is declared, a float3 to a color and back, a file name to a texture.
            (Ty::Scalar(from), Ty::Scalar(s)) => {
                let n = v.num().expect("a scalar is a number").clone();
                let n = self.convert_scalar(&n, *from, *s, origin)?;
                Ok(Val::new(to.clone(), V::Num(n)))
            }
            (Ty::Color, Ty::Vector(s, 3)) | (Ty::Vector(s, 3), Ty::Color) if s.is_floating() => {
                self.convert_val(v, to, true, origin)
            }
            (Ty::String, Ty::Texture(_) | Ty::LightProfile | Ty::BsdfMeasurement) => {
                self.construct(to, vec![v], origin)
            }
            (Ty::Scalar(_), Ty::Color) => self.construct(to, vec![v], origin),
            (Ty::Array(e, _), Ty::Array(f, Len::Deferred(_))) if e == f => Ok(Val::new(v.ty.clone(), v.v)),
            (Ty::Array(e, Len::Fixed(n)), Ty::Array(f, Len::Fixed(m))) if n == m => {
                let elements = v.elements().expect("an array").to_vec();
                let mut out = Vec::new();
                for x in elements {
                    out.push(self.convert_val(x, f, explicit, origin)?);
                }
                let _ = e;
                Ok(Val::new(to.clone(), V::Array(out)))
            }
            _ => Err(mismatch(self)),
        }
    }

    /// The cost of converting `from` to `to` without a constructor; `None` when it does not convert.
    pub fn implicit_cost(&self, from: &Ty, to: &Ty) -> Option<u32> {
        if from == to {
            return Some(0);
        }
        let scalar_cost = |a: Scalar, b: Scalar| -> Option<u32> {
            if !a.converts_to(b) {
                return None;
            }
            Some(match (a, b) {
                (Scalar::Int, Scalar::Float) | (Scalar::Float, Scalar::Double) | (Scalar::Bool, Scalar::Int) => 1,
                _ => 2,
            })
        };
        match (from, to) {
            // The SDK's compiler also narrows a float to an int where an int is declared.
            (Ty::Scalar(a), Ty::Scalar(b)) if a.is_floating() && !b.is_floating() => Some(8),
            (Ty::Scalar(a), Ty::Scalar(b)) => scalar_cost(*a, *b),
            (Ty::Vector(a, n), Ty::Vector(b, m)) if n == m => scalar_cost(*a, *b),
            (Ty::Color, Ty::Vector(s, 3)) | (Ty::Vector(s, 3), Ty::Color) if s.is_floating() => Some(4),
            (Ty::String, Ty::Texture(_) | Ty::LightProfile | Ty::BsdfMeasurement) => Some(1),
            (Ty::Scalar(_), Ty::Color) => Some(6),
            (Ty::Enum(_), Ty::Scalar(Scalar::Int)) => Some(1),
            (Ty::Enum(_), Ty::Scalar(Scalar::Float | Scalar::Double)) => Some(2),
            (Ty::Array(e, _), Ty::Array(f, Len::Deferred(_))) if e == f => Some(0),
            _ => None,
        }
    }

    // ---- operators

    /// `a op b`.
    pub fn binary(&mut self, op: BinOp, a: Val, b: Val, origin: &str) -> OpResult<Val> {
        use BinOp::*;
        if matches!(op, LogicalAnd | LogicalOr) {
            return self.logical(op, a, b, origin);
        }
        // Equality of values known at compile time: strings, enums, textures.
        if matches!(op, Eq | Ne) && !a.ty.is_numeric() && !matches!(a.ty, Ty::Matrix(..)) {
            let equal = match (&a.v, &b.v) {
                (V::Str(x), V::Str(y)) => x == y,
                (V::Enum(x), V::Enum(y)) => x == y,
                (V::Enum(x), V::Num(Num::Known(Value::Integer(y)))) => x == y,
                (V::Num(Num::Known(Value::Integer(x))), V::Enum(y)) => x == y,
                (V::Texture(x), V::Texture(y)) => x == y,
                (V::Resource(x), V::Resource(y)) => x == y,
                _ => {
                    return Err(format!("cannot compare {} with {}", self.type_name(&a.ty), self.type_name(&b.ty)));
                }
            };
            return Ok(Val::bool(equal == (op == Eq)));
        }
        let (a, b) = (self.enum_as_int(a), self.enum_as_int(b));
        if matches!(a.ty, Ty::Matrix(..)) || matches!(b.ty, Ty::Matrix(..)) {
            return self.matrix_binary(op, a, b, origin);
        }
        let (sa, sb) = match (a.ty.scalar(), b.ty.scalar()) {
            (Some(x), Some(y)) if a.ty.is_numeric() && b.ty.is_numeric() => (x, y),
            _ => {
                return Err(format!(
                    "no operator {} for {} and {}",
                    op.symbol(),
                    self.type_name(&a.ty),
                    self.type_name(&b.ty)
                ));
            }
        };
        let (wa, wb) = (a.ty.width().expect("numeric"), b.ty.width().expect("numeric"));
        let width = match (wa, wb) {
            (x, y) if x == y => x,
            (1, y) => y,
            (x, 1) => x,
            _ => return Err(format!("{} of vectors of {wa} and {wb} components", op.symbol())),
        };
        // Bitwise operators on booleans are logical ones.
        if sa == Scalar::Bool && sb == Scalar::Bool && wa == 1 && wb == 1 && matches!(op, BitAnd | BitOr | BitXor) {
            let (x, y) = (a.num().expect("a bool").clone(), b.num().expect("a bool").clone());
            let r = match op {
                BitAnd => self.and(&x, &y, origin)?,
                BitOr => self.or(&x, &y, origin)?,
                _ => self.compare(Ne, &x, &y, origin)?,
            };
            return Ok(Val::new(Ty::BOOL, V::Num(r)));
        }
        let kind = if matches!(op, Shl | Shr | ShrUnsigned | BitAnd | BitOr | BitXor) {
            Scalar::Int
        } else {
            combined(sa, sb)
        };
        let color = a.ty == Ty::Color || b.ty == Ty::Color;
        let operand_ty = if color && width == 3 {
            Ty::Color
        } else if width == 1 {
            Ty::Scalar(kind)
        } else {
            Ty::Vector(kind, width)
        };
        let comparison = matches!(op, Eq | Ne | Lt | Le | Gt | Ge);
        // Each side as the operand type: scalars stay scalars (IR ops broadcast them).
        let side = |s: &mut Self, v: Val, w: u8| -> OpResult<Val> {
            let target = if w == 1 { Ty::Scalar(kind) } else { operand_ty.clone() };
            if v.ty == Ty::Color || target == Ty::Color {
                if v.ty == target {
                    return Ok(v);
                }
                return s.convert_val(v, &target, true, origin);
            }
            s.convert_val(v, &target, true, origin)
        };
        let a = side(self, a, wa)?;
        let b = side(self, b, wb)?;
        match operand_ty.ir() {
            Some(ir) => {
                let (x, y) = (a.num().expect("a number").clone(), b.num().expect("a number").clone());
                if comparison {
                    let r = self.compare(op, &x, &y, origin)?;
                    return Ok(Val::new(Ty::BOOL, V::Num(r)));
                }
                let r = self.arith(op, &x, &y, ir, kind, origin)?;
                Ok(Val::new(operand_ty, V::Num(r)))
            }
            None => {
                // Integer and boolean vectors: component by component.
                let xs = self.spread(&a, width, origin)?;
                let ys = self.spread(&b, width, origin)?;
                let scalar_ir = if kind == Scalar::Bool { Type::Boolean } else { Type::Integer };
                let mut out = Vec::new();
                for (x, y) in xs.iter().zip(&ys) {
                    out.push(if comparison {
                        self.compare(op, x, y, origin)?
                    } else {
                        self.arith(op, x, y, scalar_ir, kind, origin)?
                    });
                }
                if comparison {
                    // `==` and `!=` of vectors compare every component into one bool.
                    let all = matches!(op, Eq);
                    let mut acc = kb(all);
                    for c in out {
                        acc = if all { self.and(&acc, &c, origin)? } else { self.or(&acc, &c, origin)? };
                    }
                    return Ok(Val::new(Ty::BOOL, V::Num(acc)));
                }
                Ok(Val::new(operand_ty, V::Parts(out)))
            }
        }
    }

    fn enum_as_int(&self, v: Val) -> Val {
        match v.v {
            V::Enum(i) => Val::int(i),
            _ => v,
        }
    }

    /// The components of `v`, a scalar spread to `width`.
    fn spread(&mut self, v: &Val, width: u8, origin: &str) -> OpResult<Vec<Num>> {
        let parts = self.parts(v, origin)?;
        Ok(if parts.len() == 1 { vec![parts[0].clone(); width as usize] } else { parts })
    }

    /// `x op y` for a comparison: one boolean.
    fn compare(&mut self, op: BinOp, x: &Num, y: &Num, origin: &str) -> OpResult<Num> {
        use BinOp::*;
        if let (Num::Known(a), Num::Known(b)) = (x, y) {
            let r = match (a, b) {
                (Value::Integer(p), Value::Integer(q)) => match op {
                    Eq => p == q,
                    Ne => p != q,
                    Lt => p < q,
                    Le => p <= q,
                    Gt => p > q,
                    _ => p >= q,
                },
                _ => {
                    let (p, q) = (a.floats(), b.floats());
                    match op {
                        Eq => p == q,
                        Ne => p != q,
                        Lt => p[0] < q[0],
                        Le => p[0] <= q[0],
                        Gt => p[0] > q[0],
                        _ => p[0] >= q[0],
                    }
                }
            };
            return Ok(kb(r));
        }
        match op {
            Eq => self.op(Op::Equal, &[x.clone(), y.clone()], Type::Boolean, origin),
            Ne => {
                let e = self.op(Op::Equal, &[x.clone(), y.clone()], Type::Boolean, origin)?;
                self.op(Op::Not, &[e], Type::Boolean, origin)
            }
            Gt => self.op(Op::Greater, &[x.clone(), y.clone()], Type::Boolean, origin),
            Ge => self.op(Op::GreaterEq, &[x.clone(), y.clone()], Type::Boolean, origin),
            Lt => self.op(Op::Greater, &[y.clone(), x.clone()], Type::Boolean, origin),
            Le => self.op(Op::GreaterEq, &[y.clone(), x.clone()], Type::Boolean, origin),
            _ => unreachable!("not a comparison"),
        }
    }

    /// `x op y` for arithmetic, of IR type `ir` and component kind `kind`.
    fn arith(&mut self, op: BinOp, x: &Num, y: &Num, ir: Type, kind: Scalar, origin: &str) -> OpResult<Num> {
        use BinOp::*;
        if kind == Scalar::Int
            && ir == Type::Integer
            && let (Some(Value::Integer(p)), Some(Value::Integer(q))) = (x.known(), y.known())
        {
            let (p, q) = (*p, *q);
            let r = match op {
                Add => p.wrapping_add(q),
                Sub => p.wrapping_sub(q),
                Mul => p.wrapping_mul(q),
                Div => {
                    if q == 0 {
                        0
                    } else {
                        p.wrapping_div(q)
                    }
                }
                Rem => {
                    if q == 0 {
                        0
                    } else {
                        p.wrapping_rem(q)
                    }
                }
                Shl => p.wrapping_shl(q as u32),
                Shr => p.wrapping_shr(q as u32),
                ShrUnsigned => ((p as u32).wrapping_shr(q as u32)) as i32,
                BitAnd => p & q,
                BitOr => p | q,
                BitXor => p ^ q,
                _ => unreachable!("not arithmetic"),
            };
            return Ok(ki(r));
        }
        match op {
            Add => self.op(Op::Add, &[x.clone(), y.clone()], ir, origin),
            Sub => self.op(Op::Subtract, &[x.clone(), y.clone()], ir, origin),
            // MaterialX multiplies no integers: in floats, exact for the products shading code makes.
            Mul if ir == Type::Integer => {
                let (fx, fy) = (self.int_to_float(x, origin)?, self.int_to_float(y, origin)?);
                let p = self.op(Op::Multiply, &[fx, fy], Type::Float, origin)?;
                self.op(Op::Floor, &[p], Type::Integer, origin)
            }
            Mul => self.op(Op::Multiply, &[x.clone(), y.clone()], ir, origin),
            Div if ir == Type::Integer => {
                // Truncating division: the float quotient converted, which truncates toward zero.
                let (fx, fy) = (self.int_to_float(x, origin)?, self.int_to_float(y, origin)?);
                let q = self.op(Op::Divide, &[fx, fy], Type::Float, origin)?;
                self.convert_scalar(&q, Scalar::Float, Scalar::Int, origin)
            }
            Div => self.op(Op::Divide, &[x.clone(), y.clone()], ir, origin),
            Rem => {
                // x - y * trunc(x / y): C's remainder, for integers and floats alike.
                let q = self.arith(Div, x, y, ir, kind, origin)?;
                let q = if ir == Type::Integer { q } else { self.trunc(&q, ir, origin)? };
                let p = self.arith(Mul, y, &q, ir, kind, origin)?;
                self.op(Op::Subtract, &[x.clone(), p], ir, origin)
            }
            Shl | Shr if y.known().is_some() && ir == Type::Integer => {
                let k = y.known().expect("known").as_f32() as i32;
                let scale = 2f32.powi(k);
                let fx = self.int_to_float(x, origin)?;
                if op == Shl {
                    let p = self.op(Op::Multiply, &[fx, kf(scale)], Type::Float, origin)?;
                    self.convert_scalar(&p, Scalar::Float, Scalar::Int, origin)
                } else {
                    // An arithmetic shift right is a floored division.
                    let q = self.op(Op::Divide, &[fx, kf(scale)], Type::Float, origin)?;
                    self.op(Op::Floor, &[q], Type::Integer, origin)
                }
            }
            // `x & (2^n - 1)`: the floored remainder by 2^n, in two's complement.
            BitAnd
                if ir == Type::Integer
                    && let Some(Value::Integer(m)) = y.known().or(x.known())
                    && *m > 0
                    && (*m as u32 + 1).is_power_of_two() =>
            {
                let a = if x.known().is_some() { y } else { x };
                let k = (*m as f32) + 1.0;
                let fa = self.int_to_float(a, origin)?;
                let q = self.op(Op::Divide, &[fa.clone(), kf(k)], Type::Float, origin)?;
                let q = self.op(Op::Floor, &[q], Type::Float, origin)?;
                let p = self.op(Op::Multiply, &[q, kf(k)], Type::Float, origin)?;
                let r = self.op(Op::Subtract, &[fa, p], Type::Float, origin)?;
                self.op(Op::Floor, &[r], Type::Integer, origin)
            }
            _ => Err(format!("{} of values known only at run time", op.symbol())),
        }
    }

    fn int_to_float(&mut self, x: &Num, origin: &str) -> OpResult<Num> {
        self.convert_scalar(x, Scalar::Int, Scalar::Float, origin)
    }

    /// `trunc(x)`: `sign(x) * floor(abs(x))`.
    pub fn trunc(&mut self, x: &Num, ir: Type, origin: &str) -> OpResult<Num> {
        let s = self.op(Op::Sign, std::slice::from_ref(x), ir, origin)?;
        let a = self.op(Op::Absval, std::slice::from_ref(x), ir, origin)?;
        let f = self.op(Op::Floor, &[a], ir, origin)?;
        self.op(Op::Multiply, &[s, f], ir, origin)
    }

    fn logical(&mut self, op: BinOp, a: Val, b: Val, origin: &str) -> OpResult<Val> {
        let a = self.convert_val(a, &Ty::BOOL, true, origin)?;
        let b = self.convert_val(b, &Ty::BOOL, true, origin)?;
        let (x, y) = (a.num().expect("a bool").clone(), b.num().expect("a bool").clone());
        let r = if op == BinOp::LogicalAnd { self.and(&x, &y, origin)? } else { self.or(&x, &y, origin)? };
        Ok(Val::new(Ty::BOOL, V::Num(r)))
    }

    pub fn and(&mut self, x: &Num, y: &Num, origin: &str) -> OpResult<Num> {
        match (x.known().map(Value::as_bool), y.known().map(Value::as_bool)) {
            (Some(false), _) | (_, Some(false)) => Ok(kb(false)),
            (Some(true), _) => Ok(y.clone()),
            (_, Some(true)) => Ok(x.clone()),
            _ if x == y => Ok(x.clone()),
            _ => self.op(Op::And, &[x.clone(), y.clone()], Type::Boolean, origin),
        }
    }

    pub fn or(&mut self, x: &Num, y: &Num, origin: &str) -> OpResult<Num> {
        match (x.known().map(Value::as_bool), y.known().map(Value::as_bool)) {
            (Some(true), _) | (_, Some(true)) => Ok(kb(true)),
            (Some(false), _) => Ok(y.clone()),
            (_, Some(false)) => Ok(x.clone()),
            _ if x == y => Ok(x.clone()),
            _ => self.op(Op::Or, &[x.clone(), y.clone()], Type::Boolean, origin),
        }
    }

    pub fn not(&mut self, x: &Num, origin: &str) -> OpResult<Num> {
        match x.known() {
            Some(v) => Ok(kb(!v.as_bool())),
            None => self.op(Op::Not, std::slice::from_ref(x), Type::Boolean, origin),
        }
    }

    /// `op v` for the prefix operators that compute (increments are assignments).
    pub fn unary(&mut self, op: UnaryOp, v: Val, origin: &str) -> OpResult<Val> {
        let v = self.enum_as_int(v);
        match op {
            UnaryOp::Plus => Ok(v),
            UnaryOp::Not => {
                let b = self.convert_val(v, &Ty::BOOL, true, origin)?;
                let n = self.not(b.num().expect("a bool"), origin)?;
                Ok(Val::new(Ty::BOOL, V::Num(n)))
            }
            UnaryOp::Neg => {
                if let Ty::Matrix(..) = v.ty {
                    return self.matrix_binary(BinOp::Mul, v, Val::float(-1.0), origin);
                }
                let zero = if v.ty.scalar() == Some(Scalar::Int) { Val::int(0) } else { Val::float(0.0) };
                self.binary(BinOp::Sub, zero, v, origin)
            }
            UnaryOp::BitNot => match v.known_int() {
                Some(i) => Ok(Val::int(!i)),
                None => Err("~ of a value known only at run time".to_string()),
            },
            UnaryOp::PreInc | UnaryOp::PreDec => unreachable!("increments are assignments"),
        }
    }

    // ---- matrices

    /// The columns of a matrix value.
    fn columns(&self, v: &Val) -> Vec<Num> {
        match &v.v {
            V::Columns(c) => c.clone(),
            _ => Vec::new(),
        }
    }

    fn matrix_binary(&mut self, op: BinOp, a: Val, b: Val, origin: &str) -> OpResult<Val> {
        use BinOp::*;
        let dims = |t: &Ty| match t {
            Ty::Matrix(_, c, r) => Some((*c, *r)),
            _ => None,
        };
        match (dims(&a.ty), dims(&b.ty), op) {
            // Componentwise: matrix with matrix of the same shape.
            (Some(da), Some(db), Add | Sub) if da == db => {
                let (ca, cb) = (self.columns(&a), self.columns(&b));
                let ir = Ty::float_vector(da.1).ir().expect("a column");
                let mut out = Vec::new();
                for (x, y) in ca.iter().zip(&cb) {
                    out.push(self.arith(op, x, y, ir, Scalar::Float, origin)?);
                }
                Ok(Val::new(a.ty.clone(), V::Columns(out)))
            }
            // Matrix product: (c1 x r1) * (c2 x r2) with c1 == r2.
            (Some((c1, r1)), Some((c2, r2)), Mul) if c1 == r2 => {
                let cb = self.columns(&b);
                let mut out = Vec::new();
                for column in cb {
                    let v = Val::new(Ty::float_vector(r2), V::Num(column));
                    let product = self.matrix_binary(Mul, a.clone(), v, origin)?;
                    out.push(product.num().expect("a vector").clone());
                }
                Ok(Val::new(Ty::Matrix(Scalar::Float, c2, r1), V::Columns(out)))
            }
            // Matrix times column vector: the columns weighted by the vector's components.
            (Some((c, r)), None, Mul) if b.ty.width() == Some(c) && c > 1 => {
                let parts = self.parts(&b, origin)?;
                let ir = Ty::float_vector(r).ir().expect("a column");
                let columns = self.columns(&a);
                let mut acc: Option<Num> = None;
                for (column, w) in columns.iter().zip(&parts) {
                    let term = self.op(Op::Multiply, &[column.clone(), w.clone()], ir, origin)?;
                    acc = Some(match acc {
                        Some(s) => self.op(Op::Add, &[s, term], ir, origin)?,
                        None => term,
                    });
                }
                Ok(Val::new(Ty::float_vector(r), V::Num(acc.expect("a column"))))
            }
            // Row vector times matrix: the vector's dot product with each column.
            (None, Some((c, r)), Mul) if a.ty.width() == Some(r) && r > 1 => {
                let x = a.num().cloned().ok_or("a row vector")?;
                let columns = self.columns(&b);
                let mut parts = Vec::new();
                for column in columns {
                    parts.push(self.op(Op::DotProduct, &[x.clone(), column], Type::Float, origin)?);
                }
                self.from_parts(&Ty::float_vector(c), parts, origin)
            }
            // Scaling by a scalar.
            (Some((_, r)), None, Mul | Div) | (None, Some((_, r)), Mul) => {
                let (m, s) = if dims(&a.ty).is_some() { (&a, &b) } else { (&b, &a) };
                let s = self.convert_val(s.clone(), &Ty::FLOAT, true, origin)?;
                let s = s.num().expect("a scalar").clone();
                let ir = Ty::float_vector(r).ir().expect("a column");
                let mut out = Vec::new();
                for column in self.columns(m) {
                    let o = if op == Div { Op::Divide } else { Op::Multiply };
                    out.push(self.op(o, &[column, s.clone()], ir, origin)?);
                }
                Ok(Val::new(m.ty.clone(), V::Columns(out)))
            }
            _ => {
                Err(format!("no operator {} for {} and {}", op.symbol(), self.type_name(&a.ty), self.type_name(&b.ty)))
            }
        }
    }

    // ---- selects

    /// `cond ? a : b` for a boolean `cond`, values of one type.
    pub fn select(&mut self, cond: &Num, a: Val, b: Val, origin: &str) -> OpResult<Val> {
        if let Some(c) = cond.known() {
            return Ok(if c.as_bool() { a } else { b });
        }
        if a == b {
            return Ok(a);
        }
        let ty = a.ty.clone();
        let v = match (a.v, b.v) {
            (V::Num(x), V::Num(y)) => V::Num(self.select_num(cond, x, y, origin)?),
            (V::Parts(x), V::Parts(y)) => V::Parts(
                x.into_iter().zip(y).map(|(p, q)| self.select_num(cond, p, q, origin)).collect::<OpResult<Vec<_>>>()?,
            ),
            (V::Columns(x), V::Columns(y)) => V::Columns(
                x.into_iter().zip(y).map(|(p, q)| self.select_num(cond, p, q, origin)).collect::<OpResult<Vec<_>>>()?,
            ),
            (V::Struct(x), V::Struct(y)) => V::Struct(
                x.into_iter().zip(y).map(|(p, q)| self.select(cond, p, q, origin)).collect::<OpResult<Vec<_>>>()?,
            ),
            (V::Array(x), V::Array(y)) if x.len() == y.len() => V::Array(
                x.into_iter().zip(y).map(|(p, q)| self.select(cond, p, q, origin)).collect::<OpResult<Vec<_>>>()?,
            ),
            (V::Df(x), V::Df(y)) => {
                let Num::Node(c) = cond else { unreachable!("a known condition was taken above") };
                V::Df(Rc::new(Df::Select { cond: *c, then: x, otherwise: y }))
            }
            (x, y) if x == y => x,
            _ => {
                return Err(format!(
                    "a {} chosen by a value known only at run time (a uniform parameter?)",
                    self.type_name(&ty)
                ));
            }
        };
        Ok(Val::new(ty, v))
    }

    /// `cond ? x : y` for numbers.
    pub fn select_num(&mut self, cond: &Num, x: Num, y: Num, origin: &str) -> OpResult<Num> {
        if let Some(c) = cond.known() {
            return Ok(if c.as_bool() { x } else { y });
        }
        if x == y {
            return Ok(x);
        }
        let ty = self.num_type(&x);
        self.op(Op::IfEqual, &[cond.clone(), kb(true), x, y], ty, origin)
    }

    // ---- constructors

    /// `ty(args)` for a type with a constructor of its own (not a struct).
    pub fn construct(&mut self, ty: &Ty, args: Vec<Val>, origin: &str) -> OpResult<Val> {
        match ty {
            Ty::Scalar(_) => match args.len() {
                0 => Ok(self.zero_of(ty)),
                1 => {
                    let a = args.into_iter().next().expect("one");
                    self.convert_val(a, ty, true, origin)
                }
                _ => Err(format!("{} takes one value", self.type_name(ty))),
            },
            Ty::Vector(..) | Ty::Color => self.construct_vector(ty, args, origin),
            Ty::Matrix(s, c, r) => self.construct_matrix(*s, *c, *r, args, origin),
            Ty::Texture(shape) => {
                let mut args = args.into_iter();
                let file = args.next().and_then(|a| a.str().map(str::to_string)).unwrap_or_default();
                let gamma = match args.next().map(|g| g.v) {
                    Some(V::Enum(1)) => Gamma::Linear,
                    Some(V::Enum(2)) => Gamma::Srgb,
                    _ => Gamma::Default,
                };
                let file = self.resolve_resource(&file);
                Ok(Val::new(ty.clone(), V::Texture(Texture { shape: *shape, file, gamma })))
            }
            Ty::LightProfile | Ty::BsdfMeasurement => {
                let file = args.first().and_then(|a| a.str().map(str::to_string)).unwrap_or_default();
                Ok(Val::new(ty.clone(), V::Resource(file)))
            }
            Ty::Bsdf | Ty::Edf | Ty::Vdf | Ty::HairBsdf if args.is_empty() => Ok(Val::df(ty.clone(), Df::Default)),
            Ty::String => Ok(args.into_iter().next().unwrap_or_else(|| Val::string(""))),
            _ => Err(format!("no constructor {} with {} arguments", self.type_name(ty), args.len())),
        }
    }

    fn zero_of(&self, ty: &Ty) -> Val {
        match ty {
            Ty::Scalar(Scalar::Bool) => Val::bool(false),
            Ty::Scalar(Scalar::Int) => Val::int(0),
            _ => Val::known(ty.clone(), Value::zero(ty.ir().expect("numeric"))),
        }
    }

    fn construct_vector(&mut self, ty: &Ty, args: Vec<Val>, origin: &str) -> OpResult<Val> {
        let n = ty.width().expect("a vector") as usize;
        let s = ty.scalar().expect("a vector");
        if args.is_empty() {
            let parts = vec![
                if s == Scalar::Bool {
                    kb(false)
                } else if s == Scalar::Int {
                    ki(0)
                } else {
                    kf(0.0)
                };
                n
            ];
            return self.from_parts(ty, parts, origin);
        }
        if args.len() == 1 {
            let a = args.into_iter().next().expect("one");
            if a.ty.width() == Some(1) {
                // A splat.
                let a = self.convert_val(a, &Ty::Scalar(s), true, origin)?;
                let p = a.num().expect("a scalar").clone();
                return self.from_parts(ty, vec![p; n], origin);
            }
            if a.ty.width() == Some(n as u8) {
                return self.convert_val(a, ty, true, origin);
            }
            if *ty == Ty::Color && matches!(a.ty, Ty::Array(..)) {
                return Err("a spectral color".to_string());
            }
            return Err(format!("{} from {}", self.type_name(ty), self.type_name(&a.ty)));
        }
        if *ty == Ty::Color && args.len() == 2 && matches!(args[0].ty, Ty::Array(..)) {
            return Err("a spectral color".to_string());
        }
        let mut parts = Vec::new();
        for a in &args {
            let from =
                a.ty.scalar()
                    .ok_or_else(|| format!("{} component from {}", self.type_name(ty), self.type_name(&a.ty)))?;
            for p in self.parts(a, origin)? {
                parts.push(self.convert_scalar(&p, from, s, origin)?);
            }
        }
        if parts.len() != n {
            return Err(format!("{} from {} components", self.type_name(ty), parts.len()));
        }
        self.from_parts(ty, parts, origin)
    }

    fn construct_matrix(&mut self, s: Scalar, c: u8, r: u8, args: Vec<Val>, origin: &str) -> OpResult<Val> {
        let ty = Ty::Matrix(s, c, r);
        let column_ty = Ty::float_vector(r);
        let ir = column_ty.ir().expect("a column");
        let zero_column = || Num::Known(Value::zero(ir));
        match args.len() {
            0 => Ok(Val::new(ty, V::Columns(vec![zero_column(); c as usize]))),
            1 if args[0].ty.width() == Some(1) => {
                // The diagonal.
                let d = self.convert_val(args[0].clone(), &Ty::FLOAT, true, origin)?;
                let d = d.num().expect("a scalar").clone();
                let mut columns = Vec::new();
                for i in 0..c {
                    let mut parts = vec![kf(0.0); r as usize];
                    if i < r {
                        parts[i as usize] = d.clone();
                    }
                    columns.push(self.from_parts(&column_ty, parts, origin)?.num().expect("a column").clone());
                }
                Ok(Val::new(ty, V::Columns(columns)))
            }
            1 if matches!(args[0].ty, Ty::Matrix(..)) => Ok(Val::new(ty, args[0].v.clone())),
            n if n == c as usize => {
                let mut columns = Vec::new();
                for a in args {
                    let a = self.convert_val(a, &column_ty, true, origin)?;
                    columns.push(a.num().expect("a column").clone());
                }
                Ok(Val::new(ty, V::Columns(columns)))
            }
            n if n == (c * r) as usize => {
                let mut columns = Vec::new();
                for chunk in args.chunks(r as usize) {
                    let mut parts = Vec::new();
                    for a in chunk {
                        let a = self.convert_val(a.clone(), &Ty::FLOAT, true, origin)?;
                        parts.push(a.num().expect("a scalar").clone());
                    }
                    columns.push(self.from_parts(&column_ty, parts, origin)?.num().expect("a column").clone());
                }
                Ok(Val::new(ty, V::Columns(columns)))
            }
            n => Err(format!("{} from {n} values", self.type_name(&ty))),
        }
    }

    /// A texture path as the host should read it: as written for the main module.
    pub fn resolve_resource(&self, file: &str) -> String {
        file.to_string()
    }

    /// An array of `elem` from `args`; `size` when written (`T[n](...)`), else as many as given.
    pub fn construct_array(&mut self, elem: &Ty, size: Option<usize>, args: Vec<Val>, origin: &str) -> OpResult<Val> {
        let mut out = Vec::new();
        for a in args {
            out.push(self.convert_val(a, elem, false, origin)?);
        }
        let n = size.unwrap_or(out.len());
        if out.len() > n {
            return Err(format!("{} values for an array of {n}", out.len()));
        }
        Ok(Val::new(Ty::Array(Box::new(elem.clone()), Len::Fixed(n)), V::Array(out)))
    }

    /// The texture shape of a texture type, for messages.
    pub fn texture_shape(ty: &Ty) -> Option<TextureShape> {
        match ty {
            Ty::Texture(s) => Some(*s),
            _ => None,
        }
    }
}
