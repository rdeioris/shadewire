use shadewire::{Op, Type};

use super::{Arg, cast};

/// Mirrors `shadewire::ops::hextile`: calls into lib/hextile.gdshaderinc, arguments in
/// `catalog::input_names` order, each cast to the type the library function takes.
pub fn emit(op: &Op, args: &[Arg], ty: Type) -> String {
    const V2: Type = Type::Vector2;
    const F: Type = Type::Float;
    let (function, tile, types): (&str, Option<u8>, &[Type]) = match op {
        Op::HextileCoord(tile) => ("shadewire_hextile_coord", Some(*tile), &[V2, F, V2, F, V2, F, V2]),
        Op::HextileGrad(tile) => ("shadewire_hextile_grad", Some(*tile), &[V2, V2, F, V2, F, V2]),
        Op::HextileWeights => ("shadewire_hextile_weights", None, &[V2, Type::Vector3, F]),
        other => panic!("{other:?} is not a hextile op"),
    };
    let mut a: Vec<String> = args.iter().zip(types).map(|((expr, from), to)| cast(expr, *from, *to)).collect();
    // The tile index follows the coordinate: `grad, coord, tile` or `coord, tile`.
    if let Some(tile) = tile {
        let at = if *op == Op::HextileGrad(tile) { 2 } else { 1 };
        a.insert(at, tile.to_string());
    }
    let result = if *op == Op::HextileWeights { Type::Vector3 } else { V2 };
    cast(&format!("{function}({})", a.join(", ")), result, ty)
}
