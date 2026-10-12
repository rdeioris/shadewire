use shadewire::{Op, Type};

use super::{Arg, cast, leading};

/// Mirrors `shadewire::ops::normal`: in, scale, normal, tangent, bitangent. A scalar
/// scale splats to vector2 (mx_normalmap_float).
pub fn emit(op: &Op, args: &[Arg], ty: Type) -> String {
    assert_eq!(*op, Op::NormalMap, "{op:?} is not a normal op");
    let scale = cast(&args[1].0, args[1].1, Type::Vector2);
    let v: Vec<String> = [0, 2, 3, 4].iter().map(|&i| leading(&args[i], 3)).collect();
    let call = format!("shadewire_normalmap({}, {scale}, {}, {}, {})", v[0], v[1], v[2], v[3]);
    cast(&call, Type::Vector3, ty)
}
