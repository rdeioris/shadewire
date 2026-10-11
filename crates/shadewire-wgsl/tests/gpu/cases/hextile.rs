//! Hextile family: tile coordinates, tile gradients and blend weights near the origin, with
//! the nodedef defaults and with every amount and range changed. The shadertoy hash
//! multiplies values that grow with the tile id (to ~50 at negative ids), where a fused
//! multiply-add moves the hash by 1e-3; tiles with ids 0 and 1 keep it under 1e-5.

use super::{Case, TRANSCENDENTAL, op_case};
use shadewire_ir::{Op, Type, Value};

const V2: Type = Type::Vector2;

/// MaterialX uv (V up): the layout space flips V, so these are tiles 0 and 1 there.
const COORDS: [[f32; 2]; 3] = [[0.1, 0.9], [0.3, 0.8], [0.45, 0.55]];

/// rotation, rotationrange, scale, scalerange, offset, offsetrange.
fn params() -> [[Value; 6]; 2] {
    [
        [
            Value::Float(1.0),
            Value::Vector2([0.0, 360.0]),
            Value::Float(1.0),
            Value::Vector2([0.5, 2.0]),
            Value::Float(1.0),
            Value::Vector2([0.0, 1.0]),
        ],
        [
            Value::Float(0.5),
            Value::Vector2([-45.0, 90.0]),
            Value::Float(0.7),
            Value::Vector2([0.8, 1.5]),
            Value::Float(0.3),
            Value::Vector2([-0.2, 0.4]),
        ],
    ]
}

pub fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    for coord in COORDS {
        let coord = Value::Vector2(coord);
        for p in params() {
            for tile in 0..3u8 {
                let args = [&[coord.clone()][..], &p].concat();
                out.push(op_case("hextile", Op::HextileCoord(tile), V2, &args, TRANSCENDENTAL));
                let args = [&[Value::Vector2([0.25, -0.5]), coord.clone()][..], &p[..4]].concat();
                out.push(op_case("hextile", Op::HextileGrad(tile), V2, &args, TRANSCENDENTAL));
            }
        }
        for falloff in [0.5, 0.75] {
            let args = [coord.clone(), Value::Vector3([0.2, 0.5, 0.9]), Value::Float(falloff)];
            out.push(op_case("hextile", Op::HextileWeights, Type::Vector3, &args, TRANSCENDENTAL));
        }
    }
    out
}
