//! Screen-space derivatives. They only feed sample gradients, which pick a mip level;
//! the interpreter evaluates one fragment with no neighbours, so every derivative is zero
//! (exact for a value that is constant across fragments, which is all const folding sees).

use crate::{Op, Type, Value};

pub fn eval(op: &Op, _args: &[Value], ty: Type) -> Value {
    match op {
        Op::Dpdx | Op::Dpdy => Value::zero(ty),
        other => panic!("{other:?} is not a derivative op"),
    }
}

#[cfg(test)]
mod tests {
    use super::eval;
    use crate::{Op, Type, Value};

    #[test]
    fn one_fragment_has_zero_derivatives() {
        assert_eq!(eval(&Op::Dpdx, &[Value::Vector2([3.0, 4.0])], Type::Vector2), Value::Vector2([0.0, 0.0]));
        assert_eq!(eval(&Op::Dpdy, &[Value::Float(2.0)], Type::Float), Value::Float(0.0));
    }
}
