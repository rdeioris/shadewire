use shadewire_ir::Type;

use super::types::wgsl_type;

/// Converts an expression between types of the same width or from a scalar to a vector.
/// 3 <-> 4 component changes are made by `Convert` ops, never here.
pub fn coerce(expr: &str, from: Type, to: Type) -> String {
    if from == to {
        return expr.to_string();
    }
    if from.is_scalar() && !to.is_scalar() {
        let scalar = match from {
            Type::Float => expr.to_string(),
            _ => format!("f32({expr})"),
        };
        return format!("{}({scalar})", wgsl_type(to));
    }
    if from.component_count() == to.component_count() {
        return expr.to_string();
    }
    panic!("coerce {from:?} -> {to:?} is a Convert, not a coercion");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splats_scalars() {
        assert_eq!(coerce("x", Type::Float, Type::Color3), "vec3<f32>(x)");
        assert_eq!(coerce("x", Type::Integer, Type::Vector2), "vec2<f32>(f32(x))");
        assert_eq!(coerce("x", Type::Boolean, Type::Vector4), "vec4<f32>(f32(x))");
    }

    #[test]
    fn same_width_is_untouched() {
        assert_eq!(coerce("x", Type::Color3, Type::Vector3), "x");
        assert_eq!(coerce("x", Type::Float, Type::Float), "x");
        assert_eq!(coerce("x", Type::Color4, Type::Vector4), "x");
    }

    #[test]
    #[should_panic(expected = "Convert")]
    fn width_changes_are_not_coercions() {
        coerce("x", Type::Color3, Type::Color4);
    }
}
