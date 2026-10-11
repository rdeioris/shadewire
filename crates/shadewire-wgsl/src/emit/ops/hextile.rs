use shadewire_ir::{Op, Type};

use super::{Arg, cast};

/// Mirrors `shadewire_ir::ops::hextile`: calls into lib/hextile.wgsl, arguments in
/// `catalog::input_names` order, each cast to the type the library function takes.
pub fn emit(op: &Op, args: &[Arg], ty: Type) -> String {
    const V2: Type = Type::Vector2;
    const F: Type = Type::Float;
    let (function, tile, types): (&str, Option<u8>, &[Type]) = match op {
        Op::HextileCoord(tile) => ("mx_hextile_coord", Some(*tile), &[V2, F, V2, F, V2, F, V2]),
        Op::HextileGrad(tile) => ("mx_hextile_grad", Some(*tile), &[V2, V2, F, V2, F, V2]),
        Op::HextileWeights => ("mx_hextile_weights", None, &[V2, Type::Vector3, F]),
        other => panic!("{other:?} is not a hextile op"),
    };
    let mut a: Vec<String> = args.iter().zip(types).map(|((expr, from), to)| cast(expr, *from, *to)).collect();
    // The tile index follows the coordinate: `grad, coord, tile` or `coord, tile`.
    if let Some(tile) = tile {
        let at = if *op == Op::HextileGrad(tile) { 2 } else { 1 };
        a.insert(at, format!("{tile}i"));
    }
    let result = if *op == Op::HextileWeights { Type::Vector3 } else { V2 };
    cast(&format!("{function}({})", a.join(", ")), result, ty)
}

#[cfg(test)]
mod tests {
    use super::emit;
    use crate::emit::test_support::args;
    use shadewire_ir::{Op, Type};

    const V2: Type = Type::Vector2;
    const F: Type = Type::Float;

    #[test]
    fn tile_index_follows_the_coordinate() {
        assert_eq!(
            emit(&Op::HextileCoord(2), &args(&[V2, F, V2, F, V2, F, V2]), V2),
            "mx_hextile_coord(n0, 2i, n1, n2, n3, n4, n5, n6)"
        );
        assert_eq!(
            emit(&Op::HextileGrad(0), &args(&[V2, V2, F, V2, F, V2]), V2),
            "mx_hextile_grad(n0, n1, 0i, n2, n3, n4, n5)"
        );
    }

    #[test]
    fn weights_take_a_vector3() {
        assert_eq!(
            emit(&Op::HextileWeights, &args(&[V2, Type::Color3, Type::Integer]), Type::Vector3),
            "mx_hextile_weights(n0, n1, f32(n2))"
        );
    }
}
