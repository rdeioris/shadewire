//! The normalmap op.
//! Ported from MaterialX libraries/stdlib/genglsl/mx_normalmap.glsl (Apache-2.0). See NOTICE.

use crate::ops::broadcast::broadcast;
use crate::{Op, Type, Value};

/// mx_normalmap_vector2 in genglsl/mx_normalmap.glsl. Arguments: in, scale, normal, tangent, bitangent.
pub fn eval(op: &Op, args: &[Value], ty: Type) -> Value {
    assert_eq!(*op, Op::NormalMap, "{op:?} is not a normal op");
    let mut value = args[0].floats();
    let scale = broadcast(&args[1], Type::Vector2).floats();
    let n = args[2].floats();
    let t = args[3].floats();
    let b = args[4].floats();
    if value.iter().map(|x| x * x).sum::<f32>() == 0.0 {
        value = vec![0.0, 0.0, 1.0];
    } else {
        for x in value.iter_mut() {
            *x = *x * 2.0 - 1.0;
        }
    }
    let mut out = [0.0f32; 3];
    for i in 0..3 {
        out[i] = t[i] * value[0] * scale[0] + b[i] * value[1] * scale[1] + n[i] * value[2];
    }
    let len = (out[0] * out[0] + out[1] * out[1] + out[2] * out[2]).sqrt();
    Value::from_floats(ty, &[out[0] / len, out[1] / len, out[2] / len])
}

#[cfg(test)]
mod tests {
    use super::eval;
    use crate::{Op, Type, Value};

    #[test]
    fn flat_normal_map_returns_the_geometric_normal() {
        let args = [Value::Vector3([0.5, 0.5, 1.0]), Value::Float(1.0), Value::Vector3([0.0, 0.0, 1.0]),
                    Value::Vector3([1.0, 0.0, 0.0]), Value::Vector3([0.0, 1.0, 0.0])];
        assert_eq!(eval(&Op::NormalMap, &args, Type::Vector3), Value::Vector3([0.0, 0.0, 1.0]));
    }

    #[test]
    fn zero_input_means_flat_and_scale_tilts() {
        let flat = [Value::Vector3([0.0, 0.0, 0.0]), Value::Float(1.0), Value::Vector3([0.0, 0.0, 1.0]),
                    Value::Vector3([1.0, 0.0, 0.0]), Value::Vector3([0.0, 1.0, 0.0])];
        assert_eq!(eval(&Op::NormalMap, &flat, Type::Vector3), Value::Vector3([0.0, 0.0, 1.0]));
        // in (1, 0.5, 1) -> (1, 0, 1); scale (2, 1) -> (2, 0, 1) / sqrt(5)
        let tilt = [Value::Vector3([1.0, 0.5, 1.0]), Value::Vector2([2.0, 1.0]), Value::Vector3([0.0, 0.0, 1.0]),
                    Value::Vector3([1.0, 0.0, 0.0]), Value::Vector3([0.0, 1.0, 0.0])];
        let out = eval(&Op::NormalMap, &tilt, Type::Vector3).floats();
        let s = 5.0f32.sqrt();
        for (a, b) in out.iter().zip([2.0 / s, 0.0, 1.0 / s]) { assert!((a - b).abs() < 1e-6); }
    }
}
