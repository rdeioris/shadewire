use shadewire::{Op, Space, TransformKind, Type};

use super::super::names::CONTEXT;
use super::{Arg, cast};

/// A space change through the matrices the context carries (`model_matrix`, `model_normal_matrix`,
/// filled from `MODEL_MATRIX` and `MODEL_NORMAL_MATRIX`). Godot has one object space, so model and
/// object are the same and a change between them is the identity. Normals are renormalized,
/// like the interpreter.
pub fn emit(op: &Op, args: &[Arg], ty: Type) -> String {
    let Op::Transform { kind, from, to } = op else {
        panic!("{op:?} is not a space op");
    };
    let v = cast(&args[0].0, args[0].1, Type::Vector3);
    let world = |s: &Space| *s == Space::World;
    let moved = match (world(from), world(to)) {
        (false, true) => match kind {
            TransformKind::Point => format!("({CONTEXT}.model_matrix * vec4({v}, 1.0)).xyz"),
            TransformKind::Vector => format!("(mat3({CONTEXT}.model_matrix) * {v})"),
            TransformKind::Normal => format!("({CONTEXT}.model_normal_matrix * {v})"),
        },
        (true, false) => match kind {
            TransformKind::Point => format!("(inverse({CONTEXT}.model_matrix) * vec4({v}, 1.0)).xyz"),
            TransformKind::Vector => format!("(inverse(mat3({CONTEXT}.model_matrix)) * {v})"),
            TransformKind::Normal => format!("(transpose(mat3({CONTEXT}.model_matrix)) * {v})"),
        },
        _ => v,
    };
    let moved = if *kind == TransformKind::Normal { format!("normalize({moved})") } else { moved };
    cast(&moved, Type::Vector3, ty)
}

/// Whether a transform needs the model matrices in the context.
pub fn needs_matrices(op: &Op) -> bool {
    matches!(op, Op::Transform { from, to, .. } if (*from == Space::World) != (*to == Space::World))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::ops::args;

    fn transform(kind: TransformKind, from: Space, to: Space) -> String {
        emit(&Op::Transform { kind, from, to }, &args(&[Type::Vector3]), Type::Vector3)
    }

    #[test]
    fn object_and_world() {
        assert_eq!(
            transform(TransformKind::Point, Space::Object, Space::World),
            "(ctx.model_matrix * vec4(n0, 1.0)).xyz"
        );
        assert_eq!(
            transform(TransformKind::Normal, Space::World, Space::Model),
            "normalize((transpose(mat3(ctx.model_matrix)) * n0))"
        );
        assert_eq!(transform(TransformKind::Vector, Space::Model, Space::Object), "n0");
        assert!(!needs_matrices(&Op::Transform { kind: TransformKind::Point, from: Space::Model, to: Space::Object }));
    }
}
