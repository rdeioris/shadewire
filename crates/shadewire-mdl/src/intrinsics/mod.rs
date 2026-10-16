//! The standard modules' functions: what each `intrinsic()` declaration computes, as IR.

mod df;
mod math;
mod state;
mod tex;

use shadewire::{Error, Type, Value};
use shadewire_nodes::Arg as NodeArg;

use crate::ast::Function;
use crate::eval::{Eval, Frame};
use crate::types::Ty;
use crate::value::{Num, V, Val};

/// Constants the compiler sets (`limits::FLOAT_MAX`, `state::WAVELENGTH_BASE_MAX`), by name.
pub fn compiler_constant(name: &str) -> Option<Val> {
    Some(match name {
        "FLOAT_MIN" => Val::float(f32::MIN_POSITIVE),
        "FLOAT_MAX" => Val::float(f32::MAX),
        "DOUBLE_MIN" => Val::known(Ty::Scalar(crate::types::Scalar::Double), Value::Float(f32::MIN_POSITIVE)),
        "DOUBLE_MAX" => Val::known(Ty::Scalar(crate::types::Scalar::Double), Value::Float(f32::MAX)),
        "WAVELENGTH_BASE_MAX" => Val::int(3),
        _ => return None,
    })
}

/// A call's arguments by parameter name.
pub struct Params<'v> {
    pub values: &'v [(String, Val)],
}

impl<'v> Params<'v> {
    pub fn get(&self, name: &str) -> &'v Val {
        &self.values.iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("no parameter {name}")).1
    }

    pub fn at(&self, i: usize) -> &'v Val {
        &self.values[i].1
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

impl<'p, 'r, 'a> Eval<'p, 'r, 'a> {
    /// A standard module's function `func`, called with `args` bound to its parameters.
    pub fn intrinsic(
        &mut self,
        f: &Frame,
        func: &Function,
        args: &[(String, Val)],
        ret: Option<Ty>,
        line: u32,
    ) -> Result<Val, Error> {
        let module = self.program.modules[f.module].name.clone();
        let ret = ret.ok_or_else(|| self.error(f, line, format!("{} returns auto", func.name)))?;
        let p = Params { values: args };
        let origin = format!("{}{}", module.trim_start_matches("::"), format_args!("::{}", func.name));
        let result = match module.as_str() {
            "::math" => self.math(&func.name, &p, &ret, &origin),
            "::state" => self.state(&func.name, &p, &ret, &origin),
            "::tex" => self.tex(&func.name, &p, &ret, &origin),
            "::df" => self.df(&func.name, &p, &ret, line, &origin),
            "::scene" => self.scene(&func.name, &p, &ret, &origin),
            "::debug" => Ok(match func.name.as_str() {
                "print" | "breakpoint" => Val::bool(true),
                _ => p.at(0).clone(),
            }),
            other => Err(format!("{other}::{} is not implemented", func.name)),
        };
        result.map_err(|m| self.error(f, line, format!("{}::{}: {m}", module.trim_start_matches("::"), func.name)))
    }

    /// `scene::data_lookup_*`: the geometry property named, with the default; `data_isvalid` is false.
    fn scene(&mut self, name: &str, p: &Params, ret: &Ty, origin: &str) -> Result<Val, String> {
        if name == "data_isvalid" {
            return Ok(Val::bool(false));
        }
        let property = p.at(0).str().unwrap_or_default().to_string();
        let default = p.at(1).clone();
        let Some(ir) = ret.ir() else {
            self.warn(format!("scene::{name}(\"{property}\"): its default"));
            return Ok(default);
        };
        let value = match default.known_value() {
            Some(v) => NodeArg::Value(v.clone()),
            None => {
                self.warn(format!("scene::{name}(\"{property}\"): a varying default read as zero"));
                NodeArg::Value(Value::zero(ir))
            }
        };
        let inputs = [("geomprop", NodeArg::Text(property)), ("default", value)];
        let id = self.e.instantiate("geompropvalue", ir, &inputs, origin).map_err(|e| e.to_string())?;
        Ok(Val::node(ret.clone(), id))
    }

    /// The IR-held number of `v`, an argument of a numeric parameter.
    pub fn n(v: &Val) -> Result<Num, String> {
        v.num().cloned().ok_or_else(|| "expected a number".to_string())
    }

    /// The IR type of the numeric type `ty`.
    pub fn ir_of(ty: &Ty) -> Result<Type, String> {
        ty.ir().ok_or_else(|| "a type without an IR value".to_string())
    }

    /// `value` as a known value of `ty`, for constants handlers return.
    pub fn known_of(ty: &Ty, x: f32) -> Val {
        match ty {
            Ty::Scalar(crate::types::Scalar::Bool) => Val::bool(x != 0.0),
            Ty::Scalar(crate::types::Scalar::Int) => Val::int(x as i32),
            _ => match ty.ir() {
                Some(ir) => Val::known(ty.clone(), Value::splat(ir, x)),
                None => Val::new(ty.clone(), V::Parts(Vec::new())),
            },
        }
    }
}
