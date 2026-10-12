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
            let out = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
            Value::from_floats(ty, &out)
        }
        // mx_rotate_vector2: `amount` in degrees.
        Op::Rotate2d => {
            let v = args[0].floats();
            let radians = args[1].as_f32() * (std::f32::consts::PI / 180.0);
            let (sa, ca) = (radians.sin(), radians.cos());
            Value::from_floats(ty, &[ca * v[0] + sa * v[1], -sa * v[0] + ca * v[1]])
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
        assert_eq!(eval(&Op::Magnitude, std::slice::from_ref(&v), Type::Float), Value::Float(5.0));
        assert_eq!(eval(&Op::Normalize, &[v], Type::Vector3), Value::Vector3([0.6, 0.0, 0.8]));
    }

    /// mx_rotate_vector2: degrees, and a positive amount turns +x towards -y.
    #[test]
    fn rotate2d_turns_by_degrees() {
        let r = eval(&Op::Rotate2d, &[Value::Vector2([1.0, 0.0]), Value::Float(90.0)], Type::Vector2).floats();
        assert!(r[0].abs() < 1e-6 && (r[1] + 1.0).abs() < 1e-6, "{r:?}");
        let r = eval(&Op::Rotate2d, &[Value::Vector2([0.6, 0.8]), Value::Float(-30.0)], Type::Vector2).floats();
        let (s, c) = (0.5f32, 0.75f32.sqrt());
        assert!((r[0] - (c * 0.6 - s * 0.8)).abs() < 1e-6 && (r[1] - (s * 0.6 + c * 0.8)).abs() < 1e-6, "{r:?}");
    }
}
