use crate::ops::broadcast::componentwise;
use crate::{Op, Type, Value};

pub fn eval(op: &Op, args: &[Value], ty: Type) -> Value {
    match op {
        Op::Sin => componentwise(args, ty, |a| a[0].sin()),
        Op::Cos => componentwise(args, ty, |a| a[0].cos()),
        Op::Tan => componentwise(args, ty, |a| a[0].tan()),
        Op::Asin => componentwise(args, ty, |a| a[0].asin()),
        Op::Acos => componentwise(args, ty, |a| a[0].acos()),
        // Arguments are iny, inx.
        Op::Atan2 => componentwise(args, ty, |a| a[0].atan2(a[1])),
        other => panic!("{other:?} is not a trig op"),
    }
}

#[cfg(test)]
mod tests {
    use super::eval;
    use crate::{Op, Type, Value};

    #[test]
    fn atan2_takes_y_first() {
        let r = eval(&Op::Atan2, &[Value::Float(1.0), Value::Float(0.0)], Type::Float);
        assert_eq!(r, Value::Float(std::f32::consts::FRAC_PI_2));
    }
}
