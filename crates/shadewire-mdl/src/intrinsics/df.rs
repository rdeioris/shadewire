//! `::df`: distribution functions are kept as calls with every parameter bound; the material's finish maps them
//! onto pbrlib lobes.

use super::Params;
use crate::eval::Eval;
use crate::types::Ty;
use crate::value::{Df, Val};

impl<'p, 'a> Eval<'p, 'a> {
    pub(super) fn df(&mut self, name: &str, p: &Params, ret: &Ty, line: u32, _origin: &str) -> Result<Val, String> {
        if ret.is_df() {
            let args = p.values.to_vec();
            return Ok(Val::df(ret.clone(), Df::Call { name: name.to_string(), args, line }));
        }
        match name {
            "light_profile_power" | "light_profile_maximum" => {
                self.warn(format!("df::{name}: 1"));
                Ok(Val::float(1.0))
            }
            "light_profile_isvalid" | "bsdf_measurement_isvalid" => Ok(Val::bool(false)),
            other => Err(format!("{other} is not implemented")),
        }
    }
}
