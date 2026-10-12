use shadewire::{Type, Value};

pub fn gd_type(ty: Type) -> &'static str {
    match ty {
        Type::Boolean => "bool",
        Type::Integer => "int",
        Type::Float => "float",
        Type::Vector2 => "vec2",
        Type::Vector3 | Type::Color3 => "vec3",
        Type::Vector4 | Type::Color4 => "vec4",
        Type::Matrix33 | Type::Matrix44 => panic!("matrix types are rejected in elaboration"),
    }
}

/// A float literal that always has a decimal point (`1.0e-7`, not `1e-7`). There are no inf or
/// nan literals, so those are built from bits; they are not constant expressions.
fn float(x: f32) -> String {
    if x.is_nan() {
        "uintBitsToFloat(2143289344u)".to_string()
    } else if x == f32::INFINITY {
        "uintBitsToFloat(2139095040u)".to_string()
    } else if x == f32::NEG_INFINITY {
        "uintBitsToFloat(4286578688u)".to_string()
    } else {
        let text = format!("{x:?}");
        match text.find('e') {
            Some(e) if !text[..e].contains('.') => format!("{}.0{}", &text[..e], &text[e..]),
            _ => text,
        }
    }
}

pub fn literal(value: &Value) -> String {
    match value {
        Value::Boolean(b) => b.to_string(),
        // -2147483648 does not parse: the magnitude overflows before the negation.
        Value::Integer(i32::MIN) => "(-2147483647 - 1)".to_string(),
        Value::Integer(i) => i.to_string(),
        Value::Float(x) => float(*x),
        Value::Matrix33(_) | Value::Matrix44(_) => panic!("matrix types are rejected in elaboration"),
        other => {
            let parts: Vec<String> = other.floats().into_iter().map(float).collect();
            format!("{}({})", gd_type(other.ty()), parts.join(", "))
        }
    }
}

/// Whether `literal(value)` is a constant expression, as a uniform default must be.
pub fn is_constant_literal(value: &Value) -> bool {
    value.floats().iter().all(|x| x.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars() {
        assert_eq!(literal(&Value::Float(1.0)), "1.0");
        assert_eq!(literal(&Value::Float(0.5)), "0.5");
        assert_eq!(literal(&Value::Float(1e-7)), "1.0e-7");
        assert_eq!(literal(&Value::Float(3.4028235e38)), "3.4028235e38");
        assert_eq!(literal(&Value::Integer(-3)), "-3");
        assert_eq!(literal(&Value::Integer(i32::MIN)), "(-2147483647 - 1)");
        assert_eq!(literal(&Value::Boolean(true)), "true");
    }

    #[test]
    fn vectors_and_non_finite() {
        assert_eq!(literal(&Value::Color3([1.0, 0.5, 0.0])), "vec3(1.0, 0.5, 0.0)");
        assert_eq!(literal(&Value::Vector2([1.0, f32::NEG_INFINITY])), "vec2(1.0, uintBitsToFloat(4286578688u))");
        assert_eq!(f32::from_bits(4286578688), f32::NEG_INFINITY);
        assert_eq!(f32::from_bits(2139095040), f32::INFINITY);
        assert!(f32::from_bits(2143289344).is_nan());
        assert!(!is_constant_literal(&Value::Float(f32::NAN)));
        assert!(is_constant_literal(&Value::Color3([1.0; 3])));
    }

    #[test]
    fn types() {
        assert_eq!(gd_type(Type::Color4), "vec4");
        assert_eq!(gd_type(Type::Integer), "int");
    }
}
