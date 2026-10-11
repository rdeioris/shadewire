use shadewire_ir::{transform_name, Op, TransformKind, Type};

use super::{cast, Arg};

/// A call to the host's `mx_transform_<kind>_<from>_to_<to>`, the name reflection lists
/// as `transform_<kind>_<from>_to_<to>`. Normals are renormalized, like the interpreter.
pub fn emit(op: &Op, args: &[Arg], ty: Type) -> String {
    let Op::Transform { kind, from, to } = op else {
        panic!("{op:?} is not a space op");
    };
    let v = cast(&args[0].0, args[0].1, Type::Vector3);
    let call = format!("mx_{}({v})", transform_name(*kind, *from, *to));
    let moved = if *kind == TransformKind::Normal { format!("normalize({call})") } else { call };
    cast(&moved, Type::Vector3, ty)
}

#[cfg(test)]
mod tests {
    use super::emit;
    use crate::emit::test_support::args;
    use shadewire_ir::{Op, Space, TransformKind, Type};

    fn transform(kind: TransformKind, from: Space) -> Op {
        Op::Transform { kind, from, to: Space::World }
    }

    #[test]
    fn host_transform_calls() {
        let a = args(&[Type::Vector3]);
        assert_eq!(emit(&transform(TransformKind::Point, Space::Model), &a, Type::Vector3), "mx_transform_point_model_to_world(n0)");
        assert_eq!(emit(&transform(TransformKind::Vector, Space::Object), &a, Type::Vector3), "mx_transform_vector_object_to_world(n0)");
        assert_eq!(
            emit(&transform(TransformKind::Normal, Space::Object), &a, Type::Vector3),
            "normalize(mx_transform_normal_object_to_world(n0))"
        );
    }
}
