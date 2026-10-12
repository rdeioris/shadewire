//! Color family: rgbtohsv, hsvtorgb (every sextant, wrapped and grey hues), their round trip, luminance.

use super::{Case, EXACT, binary, chain, unary};
use shadewire::{Op, Type, Value};

const C3: Type = Type::Color3;

pub fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    let rgbs = [
        [0.2, 0.4, 0.6],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.5],
        [0.5, 0.5, 0.5],
        [0.9, 0.1, 0.8],
        [0.0, 0.0, 0.0],
        [0.3, 0.7, 0.1],
        [0.8, 0.6, 0.2],
    ];
    for rgb in rgbs {
        out.push(unary("color", Op::RgbToHsv, C3, Value::Color3(rgb), EXACT));
        out.push(chain("color", Op::RgbToHsv, Op::HsvToRgb, C3, Value::Color3(rgb), EXACT));
    }
    let mut hsvs: Vec<[f32; 3]> = (0..6).map(|k| [k as f32 / 6.0 + 0.05, 0.8, 0.9]).collect();
    hsvs.extend([[1.5, 0.7, 0.4], [-0.2, 0.6, 0.9], [0.5, 0.00005, 0.7], [0.0, 1.0, 1.0]]);
    for hsv in &hsvs {
        out.push(unary("color", Op::HsvToRgb, C3, Value::Color3(*hsv), EXACT));
        out.push(chain("color", Op::HsvToRgb, Op::RgbToHsv, C3, Value::Color3(*hsv), EXACT));
    }
    let alpha = Value::Color4([0.25, 0.5, 0.75, 0.4]);
    out.push(unary("color", Op::RgbToHsv, Type::Color4, alpha.clone(), EXACT));
    out.push(unary("color", Op::HsvToRgb, Type::Color4, alpha.clone(), EXACT));
    let coeffs = Value::Color3([0.2126, 0.7152, 0.0722]);
    for rgb in rgbs {
        out.push(binary("color", Op::Luminance, C3, Value::Color3(rgb), coeffs.clone(), EXACT));
    }
    out.push(binary("color", Op::Luminance, Type::Color4, alpha, coeffs, EXACT));
    out
}
