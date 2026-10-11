use crate::{Op, Type, Value};

pub fn eval(op: &Op, args: &[Value], ty: Type) -> Value {
    match op {
        Op::Normalize => {
            let v = args[0].floats();
            let len = length(&v);
            let out: Vec<f32> = v.iter().map(|x| x / len).collect();
            Value::from_floats(ty, &out)
        }
        Op::Magnitude => Value::Float(length(&args[0].floats())),
        Op::Distance => {
            let a = args[0].floats();
            let b = args[1].floats();
            let diff: Vec<f32> = a.iter().zip(&b).map(|(x, y)| x - y).collect();
            Value::Float(length(&diff))
        }
        Op::DotProduct => {
            let a = args[0].floats();
            let b = args[1].floats();
            Value::Float(a.iter().zip(&b).map(|(x, y)| x * y).sum())
        }
        Op::CrossProduct => {
            let a = args[0].floats();
            let b = args[1].floats();
            let out = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            Value::from_floats(ty, &out)
        }
        other => panic!("{other:?} is not a vector op"),
    }
}

fn length(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

#[cfg(test)]
mod tests {
    use super::eval;
    use crate::{Op, Type, Value};

    #[test]
    fn cross_and_dot() {
        let x = Value::Vector3([1.0, 0.0, 0.0]);
        let y = Value::Vector3([0.0, 1.0, 0.0]);
        assert_eq!(eval(&Op::CrossProduct, &[x.clone(), y.clone()], Type::Vector3), Value::Vector3([0.0, 0.0, 1.0]));
        assert_eq!(eval(&Op::CrossProduct, &[y.clone(), x.clone()], Type::Vector3), Value::Vector3([0.0, 0.0, -1.0]));
        let z = Value::Vector3([0.0, 0.0, 1.0]);
        assert_eq!(eval(&Op::CrossProduct, &[y.clone(), z], Type::Vector3), Value::Vector3([1.0, 0.0, 0.0]));
        assert_eq!(eval(&Op::DotProduct, &[x, y], Type::Float), Value::Float(0.0));
    }

    #[test]
    fn normalize_and_magnitude() {
        let v = Value::Vector3([3.0, 0.0, 4.0]);
        assert_eq!(eval(&Op::Magnitude, &[v.clone()], Type::Float), Value::Float(5.0));
        assert_eq!(eval(&Op::Normalize, &[v], Type::Vector3), Value::Vector3([0.6, 0.0, 0.8]));
    }
}
