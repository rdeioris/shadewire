use shadewire_ir::{Op, Type};

use super::{finish, wide, Arg};

/// Mirrors `shadewire_ir::ops::derivative`: WGSL's fragment-stage `dpdx`/`dpdy`.
pub fn emit(op: &Op, args: &[Arg], ty: Type) -> String {
    let a = wide(args, ty);
    let expr = match op {
        Op::Dpdx => format!("dpdx({})", a[0]),
        Op::Dpdy => format!("dpdy({})", a[0]),
        other => panic!("{other:?} is not a derivative op"),
    };
    finish(expr, ty)
}

#[cfg(test)]
mod tests {
    use super::emit;
    use crate::emit::test_support::args;
    use shadewire_ir::{Op, Type};

    #[test]
    fn builtins() {
        assert_eq!(emit(&Op::Dpdx, &args(&[Type::Vector2]), Type::Vector2), "dpdx(n0)");
        assert_eq!(emit(&Op::Dpdy, &args(&[Type::Float]), Type::Float), "dpdy(n0)");
    }
}
