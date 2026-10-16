//! `::base` functions the evaluator does not run from their source: the noise primitives, which index permutation
//! tables at run time, as MaterialX's noise. Everything above them (octaves, turbulence, the texture_return) runs
//! as `::base` writes it.

use shadewire::{Error, Op, Type, Value};

use crate::ast::Function;
use crate::eval::{Eval, Frame};
use crate::types::{Scalar, Ty};
use crate::value::{Num, V, Val};

/// The step of the finite differences that stand in for analytic noise derivatives.
const EPSILON: f32 = 1.0e-3;

fn kf(x: f32) -> Num {
    Num::Known(Value::Float(x))
}

impl<'p, 'r, 'a> Eval<'p, 'r, 'a> {
    /// The value of a `::base` function with a handler of its own; `None` to evaluate its source.
    pub fn native(
        &mut self,
        f: &Frame,
        func: &Function,
        args: &[(String, Val)],
        ret: Option<&Ty>,
        line: u32,
    ) -> Result<Option<Val>, Error> {
        if self.program.modules[f.module].name != "::base" {
            return Ok(None);
        }
        let origin = format!("base::{}", func.name);
        let fail = |e: &Self, m: String| e.error(f, line, m);
        let first = args.first().map(|(_, v)| v.clone());
        let width = first.as_ref().and_then(|v| v.ty.width());
        let result = match (func.name.as_str(), width, args.len()) {
            ("perlin_noise", Some(_), 1) => {
                self.warn_noise("Perlin");
                let p = self.noise_position(first.as_ref().expect("an argument"), &origin)?;
                Some(Val::new(Ty::FLOAT, V::Num(self.perlin(&p, &origin)?)))
            }
            ("perlin_noise_deriv", Some(4), 1) => {
                self.warn_noise("Perlin");
                let p = self.noise_position(first.as_ref().expect("an argument"), &origin)?;
                let value = self.perlin(&p, &origin)?;
                let deriv = self.finite_difference(&p, &value, &origin, &mut |s, q, o| s.perlin(q, o))?;
                self.noise_struct(ret, &[("value", Ty::FLOAT, value), ("deriv", vec3(), deriv)], line)?
            }
            ("mi_noise", Some(3), 1) if first.as_ref().is_some_and(|v| v.ty.scalar() == Some(Scalar::Int)) => {
                self.warn_noise("cell");
                let v = first.expect("an argument");
                let v = self.convert_val(v, &vec3(), true, &origin).map_err(|m| fail(self, m))?;
                let p = v.num().expect("a vector").clone();
                let value = self.emit(Op::CellNoise3d, &[p], Type::Float, &origin)?;
                let grad = Num::Known(Value::Vector3([0.0; 3]));
                self.noise_struct(ret, &[("grad", vec3(), grad), ("val", Ty::FLOAT, value)], line)?
            }
            ("mi_noise", Some(3), 1) => {
                self.warn_noise("Perlin");
                let p = first.expect("an argument").num().expect("a vector").clone();
                let value = self.unit_perlin(&p, &origin)?;
                let grad = self.finite_difference(&p, &value, &origin, &mut |s, q, o| s.unit_perlin(q, o))?;
                self.noise_struct(ret, &[("grad", vec3(), grad), ("val", Ty::FLOAT, value)], line)?
            }
            ("worley_noise" | "worley_noise_deriv", Some(3), 4) => {
                self.warn_noise("Worley");
                let p = first.expect("an argument").num().expect("a vector").clone();
                let jitter = args[1].1.num().cloned().unwrap_or(kf(1.0));
                if args[2].1.known_int() != Some(0) {
                    self.warn("::base Worley noise with a metric other than Euclidean read as Euclidean");
                }
                if args[3].1.known_f32().is_none_or(|e| e < 1.0) {
                    self.warn("::base Worley noise edges read as cell distances");
                }
                let f1f2 = self.emit(
                    Op::WorleyNoise3d,
                    &[p.clone(), jitter, Num::Known(Value::Integer(0))],
                    Type::Vector2,
                    &origin,
                )?;
                // The nearest cells' positions are the cell centres.
                let floor = self.emit(Op::Floor, std::slice::from_ref(&p), Type::Vector3, &origin)?;
                let centre = self.emit(Op::Add, &[floor, kf(0.5)], Type::Vector3, &origin)?;
                let mut fields = vec![
                    ("nearest_pos_0", vec3(), centre.clone()),
                    ("nearest_pos_1", vec3(), centre),
                    ("val", Ty::float_vector(2), f1f2.clone()),
                ];
                if func.name == "worley_noise_deriv" {
                    let f1 = self.extract(&f1f2, 0, &origin).map_err(|m| fail(self, m))?;
                    let f2 = self.extract(&f1f2, 1, &origin).map_err(|m| fail(self, m))?;
                    let d0 = self.finite_difference(&p, &f1, &origin, &mut |s, q, o| s.worley(q, 0, o))?;
                    let d1 = self.finite_difference(&p, &f2, &origin, &mut |s, q, o| s.worley(q, 1, o))?;
                    fields.push(("deriv_0", vec3(), d0));
                    fields.push(("deriv_1", vec3(), d1));
                }
                self.noise_struct(ret, &fields, line)?
            }
            _ => None,
        };
        Ok(result)
    }

    fn warn_noise(&mut self, kind: &str) {
        self.warn(format!("::base {kind} noise approximated by MaterialX's noise"));
    }

    /// The `vector3` position of a noise argument: a float, float2 or float3 as it is (padded with zeros), a
    /// float4's time component moving the position along a fixed direction.
    fn noise_position(&mut self, v: &Val, origin: &str) -> Result<Num, Error> {
        let fail = |_: &Self, m: String| Error::BadValue { at: Default::default(), message: format!("{origin}: {m}") };
        let mut parts = self.parts(v, origin).map_err(|m| fail(self, m))?;
        if parts.len() == 4 {
            let w = parts.pop().expect("four parts");
            if w.known().is_none_or(|x| x.as_f32() != 0.0) {
                for (p, k) in parts.iter_mut().zip([0.37f32, 0.61, 0.19]) {
                    let shift = self.emit(Op::Multiply, &[w.clone(), kf(k)], Type::Float, origin)?;
                    *p = self.emit(Op::Add, &[p.clone(), shift], Type::Float, origin)?;
                }
            }
        }
        while parts.len() < 3 {
            parts.push(kf(0.0));
        }
        let v = self.from_parts(&vec3(), parts, origin).map_err(|m| fail(self, m))?;
        Ok(v.num().expect("a vector").clone())
    }

    /// Perlin noise in about [-1, 1].
    fn perlin(&mut self, p: &Num, origin: &str) -> Result<Num, Error> {
        self.emit(Op::Noise3d, &[kf(1.0), kf(0.0), p.clone()], Type::Float, origin)
    }

    /// Perlin noise in about [0, 1].
    fn unit_perlin(&mut self, p: &Num, origin: &str) -> Result<Num, Error> {
        self.emit(Op::Noise3d, &[kf(0.5), kf(0.5), p.clone()], Type::Float, origin)
    }

    /// Worley distance `index` (F1, F2).
    fn worley(&mut self, p: &Num, index: u32, origin: &str) -> Result<Num, Error> {
        let f =
            self.emit(Op::WorleyNoise3d, &[p.clone(), kf(1.0), Num::Known(Value::Integer(0))], Type::Vector2, origin)?;
        self.extract(&f, index, origin).map_err(|m| Error::BadValue { at: Default::default(), message: m })
    }

    /// The gradient of `f` at `p` by forward differences, given `f(p)`.
    fn finite_difference(
        &mut self,
        p: &Num,
        at_p: &Num,
        origin: &str,
        f: &mut dyn FnMut(&mut Self, &Num, &str) -> Result<Num, Error>,
    ) -> Result<Num, Error> {
        let mut parts = Vec::new();
        for axis in 0..3 {
            let mut d = [0.0f32; 3];
            d[axis] = EPSILON;
            let q = self.emit(Op::Add, &[p.clone(), Num::Known(Value::Vector3(d))], Type::Vector3, origin)?;
            let fq = f(self, &q, origin)?;
            let diff = self.emit(Op::Subtract, &[fq, at_p.clone()], Type::Float, origin)?;
            parts.push(self.emit(Op::Divide, &[diff, kf(EPSILON)], Type::Float, origin)?);
        }
        let v = self
            .from_parts(&vec3(), parts, origin)
            .map_err(|m| Error::BadValue { at: Default::default(), message: m })?;
        Ok(v.num().expect("a vector").clone())
    }

    /// A value of the struct `ret` with the named fields set, the rest at their defaults.
    fn noise_struct(&mut self, ret: Option<&Ty>, fields: &[(&str, Ty, Num)], line: u32) -> Result<Option<Val>, Error> {
        let Some(Ty::Struct(id)) = ret else { return Ok(None) };
        let mut given = Vec::new();
        for (name, ty, n) in fields {
            if let Some(i) = self.field_index(*id, name) {
                given.push((i, Val::new(ty.clone(), V::Num(n.clone()))));
            }
        }
        self.construct_struct(*id, given, line).map(Some)
    }
}

fn vec3() -> Ty {
    Ty::Vector(Scalar::Float, 3)
}
