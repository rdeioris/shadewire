//! Hextile family: tile coordinates, tile gradients and blend weights at tile ids 0 and 1,
//! at typical ids and at negative and large ids, with the nodedef defaults and with every
//! amount and range changed.
//!
//! The shadertoy hash starts from `fract(id * 0.1031)`, which is a fresh value in [0, 1) for
//! almost every id. It then multiplies values near 50, where a multiply-add the driver fuses
//! moves the hash by about 1e-3. That turns into about 0.4 degrees of rotation and 1e-3 uv of
//! offset per tile, and the gradients divide it by the tile scale: the coordinate and gradient
//! cases get `HEXTILE`. Ids 0 and 1 happen to stay small (drift under 1e-5). The weights use
//! no hash and keep `TRANSCENDENTAL`.

use super::{Case, TRANSCENDENTAL, op_case};
use shadewire::{Op, Type, Value};

/// Coordinates and gradients: the float hash drift, see the module docs. A gradient is also
/// divided by the tile's random scale (down to 0.5), which doubles the drift: the measured
/// worst case is 5.5e-3 (tile 1's gradient at uv (25.3, -14.2), RTX 3070 on Vulkan).
const HEXTILE: f32 = 1e-2;

const V2: Type = Type::Vector2;

/// MaterialX uv (V up). The first three are tiles 0 and 1 of the V-down layout space; then
/// typical ids, negative ids and large ids.
const COORDS: [[f32; 2]; 6] = [[0.1, 0.9], [0.3, 0.8], [0.45, 0.55], [2.1, 1.7], [-0.6, -0.9], [25.3, -14.2]];

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
                out.push(op_case("hextile", Op::HextileCoord(tile), V2, &args, HEXTILE));
                let args = [&[Value::Vector2([0.25, -0.5]), coord.clone()][..], &p[..4]].concat();
                out.push(op_case("hextile", Op::HextileGrad(tile), V2, &args, HEXTILE));
            }
        }
        for falloff in [0.5, 0.75] {
            let args = [coord.clone(), Value::Vector3([0.2, 0.5, 0.9]), Value::Float(falloff)];
            out.push(op_case("hextile", Op::HextileWeights, Type::Vector3, &args, TRANSCENDENTAL));
        }
    }
    out
}
