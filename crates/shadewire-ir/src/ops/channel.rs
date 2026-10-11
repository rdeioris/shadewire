use crate::{Op, Type, Value};

pub fn eval(op: &Op, args: &[Value], ty: Type) -> Value {
    match op {
        Op::Extract(index) => Value::Float(args[0].floats()[*index as usize]),
        Op::Combine => {
            let all: Vec<f32> = args.iter().flat_map(|a| a.floats()).collect();
            Value::from_floats(ty, &all)
        }
        Op::Convert => convert(&args[0], ty),
        other => panic!("{other:?} is not a channel op"),
    }
}

/// The ND_convert_* graphs: same width relabels, a scalar splats to every channel (alpha included),
/// 3 to 4 appends 1.0, wider to narrower drops trailing channels.
fn convert(value: &Value, ty: Type) -> Value {
    let from = value.floats();
    let want = ty.component_count();
    let out: Vec<f32> = if from.len() == 1 {
        vec![from[0]; want]
    } else if from.len() >= want {
        from[..want].to_vec()
    } else {
        let mut grown = from;
        grown.resize(want, 1.0);
        grown
    };
    Value::from_floats(ty, &out)
}

#[cfg(test)]
mod tests {
    use super::eval;
    use crate::{Op, Type, Value};

    #[test]
    fn extract_combine_convert() {
        assert_eq!(eval(&Op::Extract(2), &[Value::Color3([0.1, 0.2, 0.3])], Type::Float), Value::Float(0.3));
        assert_eq!(eval(&Op::Combine, &[Value::Color3([0.1, 0.2, 0.3]), Value::Float(0.5)], Type::Color4), Value::Color4([0.1, 0.2, 0.3, 0.5]));
        assert_eq!(eval(&Op::Convert, &[Value::Color3([0.1, 0.2, 0.3])], Type::Color4), Value::Color4([0.1, 0.2, 0.3, 1.0]));
        assert_eq!(eval(&Op::Convert, &[Value::Float(0.5)], Type::Color3), Value::Color3([0.5, 0.5, 0.5]));
    }

    #[test]
    fn convert_drops_and_relabels() {
        assert_eq!(eval(&Op::Convert, &[Value::Color4([0.1, 0.2, 0.3, 0.9])], Type::Color3), Value::Color3([0.1, 0.2, 0.3]));
        assert_eq!(eval(&Op::Convert, &[Value::Vector3([1.0, 2.0, 3.0])], Type::Vector2), Value::Vector2([1.0, 2.0]));
        assert_eq!(eval(&Op::Convert, &[Value::Vector3([1.0, 2.0, 3.0])], Type::Color3), Value::Color3([1.0, 2.0, 3.0]));
        assert_eq!(eval(&Op::Convert, &[Value::Float(0.5)], Type::Color4), Value::Color4([0.5, 0.5, 0.5, 0.5]));
        assert_eq!(eval(&Op::Convert, &[Value::Boolean(true)], Type::Float), Value::Float(1.0));
        assert_eq!(eval(&Op::Convert, &[Value::Integer(3)], Type::Float), Value::Float(3.0));
    }
}
