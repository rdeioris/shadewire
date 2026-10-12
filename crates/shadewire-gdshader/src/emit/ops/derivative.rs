use shadewire::{Op, Type};

use super::{Arg, finish, wide};

/// Mirrors `shadewire::ops::derivative`: the fragment-stage `dFdx`/`dFdy`.
pub fn emit(op: &Op, args: &[Arg], ty: Type) -> String {
    let a = wide(args, ty);
    let expr = match op {
        Op::Dpdx => format!("dFdx({})", a[0]),
        Op::Dpdy => format!("dFdy({})", a[0]),
        other => panic!("{other:?} is not a derivative op"),
    };
    finish(expr, ty)
}
