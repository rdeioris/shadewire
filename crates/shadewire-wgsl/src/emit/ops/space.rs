use shadewire::{Op, TransformKind, Type, transform_name};

use super::{Arg, cast};

/// A call to the host's `shadewire_transform_<kind>_<from>_to_<to>`, the name reflection lists
/// as `transform_<kind>_<from>_to_<to>`. Normals are renormalized, like the interpreter.
pub fn emit(op: &Op, args: &[Arg], ty: Type) -> String {
    let Op::Transform { kind, from, to } = op else {
        panic!("{op:?} is not a space op");
    };
    let v = cast(&args[0].0, args[0].1, Type::Vector3);
    let call = format!("shadewire_{}({v})", transform_name(*kind, *from, *to));
    let moved = if *kind == TransformKind::Normal { format!("normalize({call})") } else { call };
    cast(&moved, Type::Vector3, ty)
}

#[cfg(test)]
mod tests {
    use super::emit;
    use crate::emit::test_support::args;
    use shadewire::{Op, Space, TransformKind, Type};

    fn transform(kind: TransformKind, from: Space) -> Op {
        Op::Transform { kind, from, to: Space::World }
    }

    #[test]
    fn host_transform_calls() {
        let a = args(&[Type::Vector3]);
        assert_eq!(
            emit(&transform(TransformKind::Point, Space::Model), &a, Type::Vector3),
            "shadewire_transform_point_model_to_world(n0)"
        );
        assert_eq!(
            emit(&transform(TransformKind::Vector, Space::Object), &a, Type::Vector3),
            "shadewire_transform_vector_object_to_world(n0)"
        );
        assert_eq!(
            emit(&transform(TransformKind::Normal, Space::Object), &a, Type::Vector3),
            "normalize(shadewire_transform_normal_object_to_world(n0))"
        );
    }
}
