use crate::ops::broadcast::componentwise;
use crate::{Op, Type, Value};

pub fn eval(op: &Op, args: &[Value], ty: Type) -> Value {
    match op {
        Op::Add => componentwise(args, ty, |a| a[0] + a[1]),
        Op::Subtract => componentwise(args, ty, |a| a[0] - a[1]),
        // Integers multiply in two's complement, wrapping; everything else in f32.
        Op::Multiply if ty == Type::Integer => Value::Integer(int(&args[0]).wrapping_mul(int(&args[1]))),
        Op::Multiply => componentwise(args, ty, |a| a[0] * a[1]),
        Op::Divide => componentwise(args, ty, |a| a[0] / a[1]),
        Op::Modulo => componentwise(args, ty, |a| a[0] - a[1] * (a[0] / a[1]).floor()),
        Op::Invert => componentwise(args, ty, |a| a[1] - a[0]),
        Op::Absval => componentwise(args, ty, |a| a[0].abs()),
        Op::Sign => componentwise(args, ty, |a| sign(a[0])),
        Op::Floor => componentwise(args, ty, |a| a[0].floor()),
        Op::Ceil => componentwise(args, ty, |a| a[0].ceil()),
        Op::Round => componentwise(args, ty, |a| a[0].round_ties_even()),
        Op::Fract => componentwise(args, ty, |a| a[0] - a[0].floor()),
        Op::Power => componentwise(args, ty, |a| a[0].powf(a[1])),
        // MaterialX NG_safepower: sign(in1) * pow(abs(in1), in2), with sign(0) = 0.
        Op::SafePower => componentwise(args, ty, |a| sign(a[0]) * a[0].abs().powf(a[1])),
        Op::Sqrt => componentwise(args, ty, |a| a[0].sqrt()),
        Op::Exp => componentwise(args, ty, |a| a[0].exp()),
        Op::Ln => componentwise(args, ty, |a| a[0].ln()),
        Op::Min => componentwise(args, ty, |a| a[0].min(a[1])),
        Op::Max => componentwise(args, ty, |a| a[0].max(a[1])),
        Op::Clamp => componentwise(args, ty, |a| a[0].max(a[1]).min(a[2])),
        // Arguments are fg, bg, mix: genglsl `mix(bg, fg, mix)`.
        Op::Mix => componentwise(args, ty, |a| a[1] + (a[0] - a[1]) * a[2]),
        Op::Smoothstep => componentwise(args, ty, |a| smoothstep(a[1], a[2], a[0])),
        Op::Remap => componentwise(args, ty, |a| a[3] + (a[0] - a[1]) * (a[4] - a[3]) / (a[2] - a[1])),
        other => panic!("{other:?} is not a math op"),
    }
}

/// An integer argument as i32; a float one truncated (saturating), like `Value::from_floats`.
fn int(v: &Value) -> i32 {
    match v {
        Value::Integer(i) => *i,
        other => other.as_f32() as i32,
    }
}

fn sign(x: f32) -> f32 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// MaterialX `mx_smoothstep`: 0 at or below `low`, 1 at or above `high`.
fn smoothstep(low: f32, high: f32, x: f32) -> f32 {
    if x >= high {
        return 1.0;
    }
    if x <= low {
        return 0.0;
    }
    let t = (x - low) / (high - low);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::eval;
    use crate::{Op, Type, Value};

    #[test]
    fn modulo_is_floored_like_glsl_mod() {
        assert_eq!(eval(&Op::Modulo, &[Value::Float(-1.0), Value::Float(3.0)], Type::Float), Value::Float(2.0));
    }

    #[test]
    fn mix_is_bg_to_fg() {
        let r = eval(&Op::Mix, &[Value::Float(10.0), Value::Float(0.0), Value::Float(0.25)], Type::Float);
        assert_eq!(r, Value::Float(2.5));
    }

    #[test]
    fn scalar_broadcasts_over_color() {
        let r = eval(&Op::Multiply, &[Value::Color3([1.0, 2.0, 3.0]), Value::Float(2.0)], Type::Color3);
        assert_eq!(r, Value::Color3([2.0, 4.0, 6.0]));
    }

    #[test]
    fn integer_multiply_wraps() {
        let r = eval(&Op::Multiply, &[Value::Integer(3), Value::Integer(1_075_385_539)], Type::Integer);
        assert_eq!(r, Value::Integer(3i32.wrapping_mul(1_075_385_539)));
    }

    #[test]
    fn smoothstep_clamps_outside_range() {
        let lo = Value::Float(0.2);
        let hi = Value::Float(0.4);
        assert_eq!(eval(&Op::Smoothstep, &[Value::Float(0.1), lo.clone(), hi.clone()], Type::Float), Value::Float(0.0));
        assert_eq!(eval(&Op::Smoothstep, &[Value::Float(0.5), lo, hi], Type::Float), Value::Float(1.0));
    }

    #[test]
    fn round_is_half_to_even_like_wgsl() {
        assert_eq!(eval(&Op::Round, &[Value::Float(2.5)], Type::Float), Value::Float(2.0));
    }

    #[test]
    fn safepower_of_zero_is_zero() {
        assert_eq!(eval(&Op::SafePower, &[Value::Float(0.0), Value::Float(2.0)], Type::Float), Value::Float(0.0));
        assert_eq!(eval(&Op::SafePower, &[Value::Float(-2.0), Value::Float(2.0)], Type::Float), Value::Float(-4.0));
    }
}
