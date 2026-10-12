use shadewire_ir::{Op, Type};

use super::{Arg, cast, leading};

/// Mirrors `shadewire_ir::ops::normal`: in, scale, normal, tangent, bitangent. A scalar
/// scale splats to vector2 (mx_normalmap_float).
pub fn emit(op: &Op, args: &[Arg], ty: Type) -> String {
    assert_eq!(*op, Op::NormalMap, "{op:?} is not a normal op");
    let scale = cast(&args[1].0, args[1].1, Type::Vector2);
    let v: Vec<String> = [0, 2, 3, 4].iter().map(|&i| leading(&args[i], 3)).collect();
    let call = format!("mx_normalmap({}, {scale}, {}, {}, {})", v[0], v[1], v[2], v[3]);
    cast(&call, Type::Vector3, ty)
}

#[cfg(test)]
mod tests {
    use super::emit;
    use crate::emit::test_support::args;
    use shadewire_ir::{Op, Type};

    const V3: Type = Type::Vector3;

    #[test]
    fn scale_splats_and_vectors_pass_through() {
        assert_eq!(
            emit(&Op::NormalMap, &args(&[V3, Type::Float, V3, V3, V3]), V3),
            "mx_normalmap(n0, vec2<f32>(n1), n2, n3, n4)"
        );
        assert_eq!(
            emit(&Op::NormalMap, &args(&[Type::Color3, Type::Vector2, V3, V3, V3]), V3),
            "mx_normalmap(n0, n1, n2, n3, n4)"
        );
    }
}
