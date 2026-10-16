//! `::math`: one IR op where one exists, compositions of them where none does.

use shadewire::{Op, Type, Value};

use super::Params;
use crate::eval::Eval;
use crate::types::{Scalar, Ty};
use crate::value::{Num, V, Val};

type R<T> = Result<T, String>;

/// A per-component computation over the IR values of the arguments, of the IR type given.
type Componentwise<'f, S> = dyn FnMut(&mut S, &[Num], Type) -> R<Num> + 'f;

fn kf(x: f32) -> Num {
    Num::Known(Value::Float(x))
}

/// MDL's luminance weights for a `float3` (linear sRGB, CIE Y).
const LUMA: [f32; 3] = [0.212671, 0.715160, 0.072169];

impl<'p, 'a> Eval<'p, 'a> {
    fn op_n(&mut self, op: Op, args: &[Num], ty: Type, origin: &str) -> R<Num> {
        self.emit(op, args, ty, origin).map_err(|e| e.to_string())
    }

    /// `f` over each component of the numeric arguments: once over the IR values when the result is held as one,
    /// else per component (integer vectors).
    fn componentwise(&mut self, args: &[&Val], ret: &Ty, origin: &str, f: &mut Componentwise<'_, Self>) -> R<Val> {
        if let Some(ir) = ret.ir() {
            let nums: Vec<Num> = args.iter().map(|a| Self::n(a)).collect::<R<_>>()?;
            return Ok(Val::new(ret.clone(), V::Num(f(self, &nums, ir)?)));
        }
        let width = ret.width().ok_or("a numeric result")? as usize;
        let scalar_ir = match ret.scalar() {
            Some(Scalar::Bool) => Type::Boolean,
            Some(Scalar::Int) => Type::Integer,
            _ => Type::Float,
        };
        let mut columns: Vec<Vec<Num>> = Vec::new();
        for a in args {
            let parts = self.parts(a, origin)?;
            columns.push(if parts.len() == 1 { vec![parts[0].clone(); width] } else { parts });
        }
        let mut out = Vec::new();
        for c in 0..width {
            let nums: Vec<Num> = columns.iter().map(|col| col[c].clone()).collect();
            out.push(f(self, &nums, scalar_ir)?);
        }
        Ok(Val::new(ret.clone(), V::Parts(out)))
    }

    /// `op` componentwise over every parameter, in order.
    fn simple(&mut self, op: Op, p: &Params, ret: &Ty, origin: &str) -> R<Val> {
        let args: Vec<&Val> = p.values.iter().map(|(_, v)| v).collect();
        self.componentwise(&args, ret, origin, &mut |s, n, ty| s.op_n(op, n, ty, origin))
    }

    /// The components of `v` reduced by `op` (`add`, `max`, ...).
    fn reduce(&mut self, v: &Val, op: Op, origin: &str) -> R<Num> {
        let parts = self.parts(v, origin)?;
        let ty = if v.ty.scalar() == Some(Scalar::Int) { Type::Integer } else { Type::Float };
        let mut acc = parts[0].clone();
        for p in &parts[1..] {
            acc = self.op_n(op, &[acc, p.clone()], ty, origin)?;
        }
        Ok(acc)
    }

    pub(super) fn math(&mut self, name: &str, p: &Params, ret: &Ty, origin: &str) -> R<Val> {
        let o = origin;
        match name {
            "abs" => self.simple(Op::Absval, p, ret, o),
            "sin" => self.simple(Op::Sin, p, ret, o),
            "cos" => self.simple(Op::Cos, p, ret, o),
            "tan" => self.simple(Op::Tan, p, ret, o),
            "asin" => self.simple(Op::Asin, p, ret, o),
            "acos" => self.simple(Op::Acos, p, ret, o),
            "floor" => self.simple(Op::Floor, p, ret, o),
            "ceil" => self.simple(Op::Ceil, p, ret, o),
            "round" => self.simple(Op::Round, p, ret, o),
            "sign" => self.simple(Op::Sign, p, ret, o),
            "sqrt" => self.simple(Op::Sqrt, p, ret, o),
            "exp" => self.simple(Op::Exp, p, ret, o),
            "log" => self.simple(Op::Ln, p, ret, o),
            "frac" => self.simple(Op::Fract, p, ret, o),
            "min" => self.simple(Op::Min, p, ret, o),
            "max" => self.simple(Op::Max, p, ret, o),
            "pow" => self.simple(Op::Power, p, ret, o),
            "clamp" => self.simple(Op::Clamp, p, ret, o),
            "atan2" => self.simple(Op::Atan2, p, ret, o),
            "cross" => self.simple(Op::CrossProduct, p, ret, o),
            "normalize" => self.simple(Op::Normalize, p, ret, o),
            "DX" => self.simple(Op::Dpdx, p, ret, o),
            "DY" => self.simple(Op::Dpdy, p, ret, o),
            "dot" | "distance" | "length" => {
                let op = match name {
                    "dot" => Op::DotProduct,
                    "distance" => Op::Distance,
                    _ => Op::Magnitude,
                };
                let args: Vec<Num> = p.values.iter().map(|(_, v)| Self::n(v)).collect::<R<_>>()?;
                // A scalar's length and distance are its absolute value and difference.
                if p.at(0).ty.width() == Some(1) {
                    let v = match op {
                        Op::DotProduct => self.op_n(Op::Multiply, &args, Type::Float, o)?,
                        Op::Distance => {
                            let d = self.op_n(Op::Subtract, &args, Type::Float, o)?;
                            self.op_n(Op::Absval, &[d], Type::Float, o)?
                        }
                        _ => self.op_n(Op::Absval, &args, Type::Float, o)?,
                    };
                    return Ok(Val::new(ret.clone(), V::Num(v)));
                }
                Ok(Val::new(ret.clone(), V::Num(self.op_n(op, &args, Type::Float, o)?)))
            }
            "atan" => self
                .componentwise(&[p.at(0)], ret, o, &mut |s, n, ty| s.op_n(Op::Atan2, &[n[0].clone(), kf(1.0)], ty, o)),
            "exp2" => self
                .componentwise(&[p.at(0)], ret, o, &mut |s, n, ty| s.op_n(Op::Power, &[kf(2.0), n[0].clone()], ty, o)),
            "log2" | "log10" => {
                let k = if name == "log2" { std::f32::consts::LN_2 } else { std::f32::consts::LN_10 };
                self.componentwise(&[p.at(0)], ret, o, &mut |s, n, ty| {
                    let l = s.op_n(Op::Ln, &n[..1], ty, o)?;
                    s.op_n(Op::Divide, &[l, kf(k)], ty, o)
                })
            }
            "rsqrt" => self.componentwise(&[p.at(0)], ret, o, &mut |s, n, ty| {
                let r = s.op_n(Op::Sqrt, &n[..1], ty, o)?;
                s.op_n(Op::Divide, &[kf(1.0), r], ty, o)
            }),
            "saturate" => self.componentwise(&[p.at(0)], ret, o, &mut |s, n, ty| {
                s.op_n(Op::Clamp, &[n[0].clone(), kf(0.0), kf(1.0)], ty, o)
            }),
            "radians" | "degrees" => {
                let k = if name == "radians" { std::f32::consts::PI / 180.0 } else { 180.0 / std::f32::consts::PI };
                self.componentwise(&[p.at(0)], ret, o, &mut |s, n, ty| {
                    s.op_n(Op::Multiply, &[n[0].clone(), kf(k)], ty, o)
                })
            }
            "lerp" => self.componentwise(&[p.get("a"), p.get("b"), p.get("l")], ret, o, &mut |s, n, ty| {
                s.op_n(Op::Mix, &[n[1].clone(), n[0].clone(), n[2].clone()], ty, o)
            }),
            "smoothstep" => self.componentwise(&[p.get("a"), p.get("b"), p.get("l")], ret, o, &mut |s, n, ty| {
                s.op_n(Op::Smoothstep, &[n[2].clone(), n[0].clone(), n[1].clone()], ty, o)
            }),
            "step" => {
                // 0 where b < a, else 1, per component.
                let (a, b) = (p.get("a").clone(), p.get("b").clone());
                let width = ret.width().unwrap_or(1) as usize;
                let (pa, pb) = (self.parts(&a, o)?, self.parts(&b, o)?);
                let mut out = Vec::new();
                for c in 0..width {
                    let (x, y) = (pa[c.min(pa.len() - 1)].clone(), pb[c.min(pb.len() - 1)].clone());
                    out.push(self.op_n(Op::IfGreaterEq, &[y, x, kf(1.0), kf(0.0)], Type::Float, o)?);
                }
                self.from_parts(ret, out, o)
            }
            "fmod" => self.componentwise(&[p.get("a"), p.get("b")], ret, o, &mut |s, n, ty| {
                let q = s.op_n(Op::Divide, &[n[0].clone(), n[1].clone()], ty, o)?;
                let t = s.trunc(&q, ty, o)?;
                let m = s.op_n(Op::Multiply, &[n[1].clone(), t], ty, o)?;
                s.op_n(Op::Subtract, &[n[0].clone(), m], ty, o)
            }),
            "round_away_from_zero" => self.componentwise(&[p.at(0)], ret, o, &mut |s, n, ty| {
                let sign = s.op_n(Op::Sign, &n[..1], ty, o)?;
                let a = s.op_n(Op::Absval, &n[..1], ty, o)?;
                let h = s.op_n(Op::Add, &[a, kf(0.5)], ty, o)?;
                let f = s.op_n(Op::Floor, &[h], ty, o)?;
                s.op_n(Op::Multiply, &[sign, f], ty, o)
            }),
            "sinh" | "cosh" | "tanh" => self.componentwise(&[p.at(0)], ret, o, &mut |s, n, ty| {
                let e = s.op_n(Op::Exp, &n[..1], ty, o)?;
                let neg = s.op_n(Op::Multiply, &[n[0].clone(), kf(-1.0)], ty, o)?;
                let m = s.op_n(Op::Exp, &[neg], ty, o)?;
                match name {
                    "sinh" => {
                        let d = s.op_n(Op::Subtract, &[e, m], ty, o)?;
                        s.op_n(Op::Multiply, &[d, kf(0.5)], ty, o)
                    }
                    "cosh" => {
                        let d = s.op_n(Op::Add, &[e, m], ty, o)?;
                        s.op_n(Op::Multiply, &[d, kf(0.5)], ty, o)
                    }
                    _ => {
                        let num = s.op_n(Op::Subtract, &[e.clone(), m.clone()], ty, o)?;
                        let den = s.op_n(Op::Add, &[e, m], ty, o)?;
                        s.op_n(Op::Divide, &[num, den], ty, o)
                    }
                }
            }),
            "average" => {
                let w = p.at(0).ty.width().unwrap_or(1) as f32;
                let sum = self.reduce(p.at(0), Op::Add, o)?;
                let sum = self.convert_scalar(&sum, p.at(0).ty.scalar().unwrap_or(Scalar::Float), Scalar::Float, o)?;
                Ok(Val::new(ret.clone(), V::Num(self.op_n(Op::Divide, &[sum, kf(w)], Type::Float, o)?)))
            }
            "max_value" | "min_value" => {
                let op = if name == "max_value" { Op::Max } else { Op::Min };
                Ok(Val::new(ret.clone(), V::Num(self.reduce(p.at(0), op, o)?)))
            }
            "luminance" => {
                let x = Self::n(p.at(0))?;
                let x =
                    if self.num_type(&x) == Type::Color3 { self.op_n(Op::Convert, &[x], Type::Vector3, o)? } else { x };
                let w = Num::Known(Value::Vector3(LUMA));
                Ok(Val::new(ret.clone(), V::Num(self.op_n(Op::DotProduct, &[x, w], Type::Float, o)?)))
            }
            "all" | "any" => {
                let parts = self.parts(p.at(0), o)?;
                let mut acc = Num::Known(Value::Boolean(name == "all"));
                for c in parts {
                    acc = if name == "all" { self.and(&acc, &c, o)? } else { self.or(&acc, &c, o)? };
                }
                Ok(Val::new(ret.clone(), V::Num(acc)))
            }
            "isnan" | "isfinite" => {
                let w = p.at(0).ty.width().unwrap_or(1);
                let b = name == "isfinite";
                let ty = if w == 1 { Ty::BOOL } else { Ty::Vector(Scalar::Bool, w) };
                Ok(if w == 1 {
                    Val::bool(b)
                } else {
                    Val::new(ty, V::Parts(vec![Num::Known(Value::Boolean(b)); w as usize]))
                })
            }
            "sincos" | "modf" => {
                let x = p.at(0).clone();
                let ty = x.ty.clone();
                let (a, b) = if name == "sincos" {
                    let s = self.componentwise(&[&x], &ty, o, &mut |s, n, t| s.op_n(Op::Sin, &n[..1], t, o))?;
                    let c = self.componentwise(&[&x], &ty, o, &mut |s, n, t| s.op_n(Op::Cos, &n[..1], t, o))?;
                    (s, c)
                } else {
                    let i = self.componentwise(&[&x], &ty, o, &mut |s, n, t| s.trunc(&n[0], t, o))?;
                    let fr = self.binary(crate::ast::BinOp::Sub, x.clone(), i.clone(), o)?;
                    (i, fr)
                };
                Ok(Val::new(ret.clone(), V::Array(vec![a, b])))
            }
            "transpose" => {
                let V::Columns(cols) = &p.at(0).v else { return Err("a matrix".into()) };
                let Ty::Matrix(_, c, r) = ret else { return Err("a matrix".into()) };
                let source = p.at(0).clone();
                let mut out = Vec::new();
                for j in 0..*c {
                    let mut parts = Vec::new();
                    for i in 0..*r {
                        let col = Val::new(Ty::float_vector(*c), V::Num(cols[i as usize].clone()));
                        parts.push(self.parts(&col, o)?[j as usize].clone());
                    }
                    let column = self.from_parts(&Ty::float_vector(*r), parts, o)?;
                    out.push(column.num().expect("a column").clone());
                }
                let _ = source;
                Ok(Val::new(ret.clone(), V::Columns(out)))
            }
            "blackbody" => match p.at(0).known_f32() {
                Some(t) => Ok(Val::known(Ty::Color, Value::Color3(blackbody(t)))),
                None => {
                    self.warn("math::blackbody of a temperature known only at run time: 6500 K");
                    Ok(Val::known(Ty::Color, Value::Color3(blackbody(6500.0))))
                }
            },
            "emission_color" => {
                if p.len() == 1 {
                    return Ok(p.at(0).clone());
                }
                self.warn("math::emission_color of a spectrum: white");
                Ok(Val::known(Ty::Color, Value::Color3([1.0; 3])))
            }
            "eval_at_wavelength" => {
                self.warn("math::eval_at_wavelength: the color's luminance");
                let x = Self::n(p.at(0))?;
                let x = self.op_n(Op::Convert, &[x], Type::Vector3, o)?;
                let w = Num::Known(Value::Vector3(LUMA));
                Ok(Val::new(ret.clone(), V::Num(self.op_n(Op::DotProduct, &[x, w], Type::Float, o)?)))
            }
            "max_value_wavelength" | "min_value_wavelength" => {
                Ok(Val::float(if name.starts_with("max") { 780.0 } else { 380.0 }))
            }
            "float_bits_to_int" => match p.at(0).known_f32() {
                Some(x) => Ok(Val::int(x.to_bits() as i32)),
                None => Err("of a value known only at run time".into()),
            },
            "int_bits_to_float" => match p.at(0).known_int() {
                Some(i) => Ok(Val::float(f32::from_bits(i as u32))),
                None => Err("of a value known only at run time".into()),
            },
            other => Err(format!("{other} is not implemented")),
        }
    }
}

/// The linear sRGB color of a black body at `kelvin`, normalized to a luminance of one (Planck's law sampled over
/// the CIE 1931 observer's piecewise Gaussian fit).
pub fn blackbody(kelvin: f32) -> [f32; 3] {
    let t = (kelvin as f64).max(1.0);
    let g = |x: f64, mu: f64, s1: f64, s2: f64| {
        let s = if x < mu { s1 } else { s2 };
        (-0.5 * ((x - mu) / s).powi(2)).exp()
    };
    let (mut x, mut y, mut z) = (0.0f64, 0.0f64, 0.0f64);
    let mut l = 380.0f64;
    while l <= 780.0 {
        let xb = 1.056 * g(l, 599.8, 37.9, 31.0) + 0.362 * g(l, 442.0, 16.0, 26.7) - 0.065 * g(l, 501.1, 20.4, 26.2);
        let yb = 0.821 * g(l, 568.8, 46.9, 40.5) + 0.286 * g(l, 530.9, 16.3, 31.1);
        let zb = 1.217 * g(l, 437.0, 11.8, 36.0) + 0.681 * g(l, 459.0, 26.0, 13.8);
        let m = l * 1e-9;
        let planck = 1.0 / (m.powi(5) * ((1.438_776_9e-2 / (m * t)).exp() - 1.0));
        x += xb * planck;
        y += yb * planck;
        z += zb * planck;
        l += 5.0;
    }
    let (x, z) = (x / y, z / y);
    let r = 3.240_454_2 * x - 1.537_138_5 - 0.498_531_4 * z;
    let gr = -0.969_266 * x + 1.876_010_8 + 0.041_556 * z;
    let b = 0.055_643_4 * x - 0.204_025_9 + 1.057_225_2 * z;
    [r.max(0.0) as f32, gr.max(0.0) as f32, b.max(0.0) as f32]
}
