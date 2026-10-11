use crate::ops::broadcast::broadcast;
use crate::{Op, Type, Value};

pub fn eval(op: &Op, args: &[Value], ty: Type) -> Value {
    match op {
        Op::Greater => Value::Boolean(args[0].as_f32() > args[1].as_f32()),
        Op::GreaterEq => Value::Boolean(args[0].as_f32() >= args[1].as_f32()),
        Op::Equal => Value::Boolean(args[0].floats() == args[1].floats()),
        Op::IfGreater => pick(args[0].as_f32() > args[1].as_f32(), &args[2], &args[3], ty),
        Op::IfGreaterEq => pick(args[0].as_f32() >= args[1].as_f32(), &args[2], &args[3], ty),
        Op::IfEqual => pick(args[0].floats() == args[1].floats(), &args[2], &args[3], ty),
        Op::Switch => switch(args, ty),
        Op::And => Value::Boolean(args[0].as_bool() && args[1].as_bool()),
        Op::Or => Value::Boolean(args[0].as_bool() || args[1].as_bool()),
        Op::Not => Value::Boolean(!args[0].as_bool()),
        other => panic!("{other:?} is not a logic op"),
    }
}

fn pick(condition: bool, when_true: &Value, when_false: &Value, ty: Type) -> Value {
    if condition { broadcast(when_true, ty) } else { broadcast(when_false, ty) }
}

/// NG_switch_* in stdlib_ng.mtlx: a chain of `ifgreater(k, which)` for k = 1..10 picking in_k,
/// ending in zero. So which < 1 gives in1, which >= 10 gives zero.
fn switch(args: &[Value], ty: Type) -> Value {
    let which = args[10].as_f32();
    for k in 1..=10 {
        if (k as f32) > which {
            return broadcast(&args[k - 1], ty);
        }
    }
    Value::zero(ty)
}

#[cfg(test)]
mod tests {
    use super::eval;
    use crate::{Op, Type, Value};

    #[test]
    fn ifgreater_picks_in1_when_value1_is_greater() {
        let args =
            [Value::Float(2.0), Value::Float(1.0), Value::Color3([1.0, 0.0, 0.0]), Value::Color3([0.0, 1.0, 0.0])];
        assert_eq!(eval(&Op::IfGreater, &args, Type::Color3), Value::Color3([1.0, 0.0, 0.0]));
    }

    #[test]
    fn ifequal_and_ifgreatereq_take_the_equal_case() {
        let args = [Value::Float(1.0), Value::Float(1.0), Value::Float(5.0), Value::Float(6.0)];
        assert_eq!(eval(&Op::IfEqual, &args, Type::Float), Value::Float(5.0));
        assert_eq!(eval(&Op::IfGreaterEq, &args, Type::Float), Value::Float(5.0));
        assert_eq!(eval(&Op::IfGreater, &args, Type::Float), Value::Float(6.0));
    }

    #[test]
    fn booleans() {
        let t = Value::Boolean(true);
        let f = Value::Boolean(false);
        assert_eq!(eval(&Op::And, &[t.clone(), f.clone()], Type::Boolean), Value::Boolean(false));
        assert_eq!(eval(&Op::Or, &[t.clone(), f.clone()], Type::Boolean), Value::Boolean(true));
        assert_eq!(eval(&Op::Not, &[f], Type::Boolean), Value::Boolean(true));
        assert_eq!(eval(&Op::Greater, &[Value::Float(2.0), Value::Float(1.0)], Type::Boolean), Value::Boolean(true));
        assert_eq!(eval(&Op::GreaterEq, &[Value::Float(1.0), Value::Float(1.0)], Type::Boolean), Value::Boolean(true));
        assert_eq!(eval(&Op::Equal, &[Value::Float(1.0), Value::Float(2.0)], Type::Boolean), Value::Boolean(false));
    }

    // MaterialX's switch is a chain of ifgreater(k, which): which < 1 gives in1,
    // [k-1, k) gives in_k, and which >= 10 falls off the end to zero.
    #[test]
    fn switch_follows_the_materialx_chain() {
        let mut args: Vec<Value> = (1..=10).map(|i| Value::Float(i as f32)).collect();
        args.push(Value::Float(2.7));
        assert_eq!(eval(&Op::Switch, &args, Type::Float), Value::Float(3.0));
        *args.last_mut().unwrap() = Value::Float(-4.0);
        assert_eq!(eval(&Op::Switch, &args, Type::Float), Value::Float(1.0));
        *args.last_mut().unwrap() = Value::Float(9.5);
        assert_eq!(eval(&Op::Switch, &args, Type::Float), Value::Float(10.0));
        *args.last_mut().unwrap() = Value::Float(42.0);
        assert_eq!(eval(&Op::Switch, &args, Type::Float), Value::Float(0.0));
    }
}
