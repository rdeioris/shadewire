use shadewire::{Type, Value};

pub fn wgsl_type(ty: Type) -> &'static str {
    match ty {
        Type::Boolean => "bool",
        Type::Integer => "i32",
        Type::Float => "f32",
        Type::Vector2 => "vec2<f32>",
        Type::Vector3 | Type::Color3 => "vec3<f32>",
        Type::Vector4 | Type::Color4 => "vec4<f32>",
        Type::Matrix33 | Type::Matrix44 => panic!("matrix types are rejected in elaboration"),
    }
}

/// An f32 literal. WGSL has no inf or nan literals, so those are built from bits.
fn float(x: f32) -> String {
    if x.is_nan() {
        "bitcast<f32>(0x7fc00000u)".to_string()
    } else if x == f32::INFINITY {
        "bitcast<f32>(0x7f800000u)".to_string()
    } else if x == f32::NEG_INFINITY {
        "bitcast<f32>(0xff800000u)".to_string()
    } else {
        format!("{x:?}")
    }
}

pub fn literal(value: &Value) -> String {
    match value {
        Value::Boolean(b) => b.to_string(),
        // -2147483648i does not parse: the magnitude overflows before the negation.
        Value::Integer(i32::MIN) => "i32(-2147483647) - 1i".to_string(),
        Value::Integer(i) => format!("{i}i"),
        Value::Float(x) => float(*x),
        Value::Matrix33(_) | Value::Matrix44(_) => {
            panic!("matrix types are rejected in elaboration")
        }
        other => {
            let parts: Vec<String> = other.floats().into_iter().map(float).collect();
            format!("{}({})", wgsl_type(other.ty()), parts.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars() {
        assert_eq!(literal(&Value::Float(1.0)), "1.0");
        assert_eq!(literal(&Value::Float(0.5)), "0.5");
        assert_eq!(literal(&Value::Float(1e-7)), "1e-7");
        assert_eq!(literal(&Value::Integer(-3)), "-3i");
        assert_eq!(literal(&Value::Boolean(true)), "true");
    }

    #[test]
    fn vectors() {
        assert_eq!(literal(&Value::Color3([1.0, 0.5, 0.0])), "vec3<f32>(1.0, 0.5, 0.0)");
        assert_eq!(literal(&Value::Vector2([1.0, 2.0])), "vec2<f32>(1.0, 2.0)");
    }

    #[test]
    fn types() {
        assert_eq!(wgsl_type(Type::Color4), "vec4<f32>");
        assert_eq!(wgsl_type(Type::Integer), "i32");
    }

    #[test]
    fn non_finite_floats_parse_in_naga() {
        let cases = [
            (f32::INFINITY, "bitcast<f32>(0x7f800000u)"),
            (f32::NEG_INFINITY, "bitcast<f32>(0xff800000u)"),
            (f32::NAN, "bitcast<f32>(0x7fc00000u)"),
        ];
        for (x, text) in cases {
            assert_eq!(literal(&Value::Float(x)), text);
            let src = format!("fn f() -> f32 {{ return {}; }}", literal(&Value::Float(x)));
            naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{src}: {e:?}"));
        }
        let v = literal(&Value::Vector3([f32::INFINITY, 1.0, f32::NAN]));
        let src = format!("fn f() -> vec3<f32> {{ return {v}; }}");
        naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{src}: {e:?}"));
    }

    fn parse_f32_bits(x: f32) -> u32 {
        let src = format!("const C: f32 = {};", literal(&Value::Float(x)));
        let module = naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{src}: {e:?}"));
        let (_, c) = module.constants.iter().next().unwrap();
        match module.global_expressions[c.init] {
            naga::Expression::Literal(naga::Literal::F32(v)) => v.to_bits(),
            ref other => panic!("{src}: not folded to a literal: {other:?}"),
        }
    }

    #[test]
    fn edge_floats_keep_their_bits() {
        let subnormal = f32::from_bits(1);
        for x in [-0.0f32, 3.4028235e38, -3.4028235e38, subnormal, 1.1754942e-38] {
            assert_eq!(parse_f32_bits(x), x.to_bits(), "{x:?} -> {}", literal(&Value::Float(x)));
        }
    }

    #[test]
    fn i32_min_parses() {
        let text = literal(&Value::Integer(i32::MIN));
        assert_eq!(text, "i32(-2147483647) - 1i");
        let src = format!("const C: i32 = {text};");
        let module = naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{src}: {e:?}"));
        let (_, c) = module.constants.iter().next().unwrap();
        assert!(matches!(module.global_expressions[c.init], naga::Expression::Literal(naga::Literal::I32(i32::MIN))));
    }
}
