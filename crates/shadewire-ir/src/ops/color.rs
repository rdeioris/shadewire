use crate::{Op, Type, Value};

pub fn eval(op: &Op, args: &[Value], ty: Type) -> Value {
    match op {
        Op::Luminance => {
            let rgb = args[0].floats();
            let coeffs = args[1].floats();
            let l = rgb[0] * coeffs[0] + rgb[1] * coeffs[1] + rgb[2] * coeffs[2];
            if ty == Type::Color4 {
                Value::Color4([l, l, l, rgb[3]])
            } else {
                Value::Color3([l, l, l])
            }
        }
        Op::RgbToHsv => map_rgb(&args[0], ty, rgb_to_hsv),
        Op::HsvToRgb => map_rgb(&args[0], ty, hsv_to_rgb),
        other => panic!("{other:?} is not a color op"),
    }
}

/// Applies `f` to the first three channels and keeps alpha for color4.
fn map_rgb(value: &Value, ty: Type, f: fn([f32; 3]) -> [f32; 3]) -> Value {
    let c = value.floats();
    let out = f([c[0], c[1], c[2]]);
    if ty == Type::Color4 {
        Value::Color4([out[0], out[1], out[2], c[3]])
    } else {
        Value::Color3(out)
    }
}

/// mx_hsvtorgb in genglsl/lib/mx_hsv.glsl.
fn hsv_to_rgb(hsv: [f32; 3]) -> [f32; 3] {
    let (mut h, s, v) = (hsv[0], hsv[1], hsv[2]);
    if s < 0.0001 {
        return [v, v, v];
    }
    h = 6.0 * (h - h.floor());
    let hi = h.trunc() as i32;
    let f = h - hi as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match hi {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

/// mx_rgbtohsv in genglsl/lib/mx_hsv.glsl.
fn rgb_to_hsv(c: [f32; 3]) -> [f32; 3] {
    let (r, g, b) = (c[0], c[1], c[2]);
    let mincomp = r.min(g.min(b));
    let maxcomp = r.max(g.max(b));
    let delta = maxcomp - mincomp;
    let v = maxcomp;
    let s = if maxcomp > 0.0 { delta / maxcomp } else { 0.0 };
    let mut h;
    if s <= 0.0 {
        h = 0.0;
    } else {
        h = if r >= maxcomp {
            (g - b) / delta
        } else if g >= maxcomp {
            2.0 + (b - r) / delta
        } else {
            4.0 + (r - g) / delta
        };
        h *= 1.0 / 6.0;
        if h < 0.0 {
            h += 1.0;
        }
    }
    [h, s, v]
}

#[cfg(test)]
mod tests {
    use super::eval;
    use crate::{Op, Type, Value};

    #[test]
    fn hsv_round_trip() {
        let c = Value::Color3([0.2, 0.6, 0.4]);
        let hsv = eval(&Op::RgbToHsv, std::slice::from_ref(&c), Type::Color3);
        let back = eval(&Op::HsvToRgb, &[hsv], Type::Color3).floats();
        for (a, b) in back.iter().zip(c.floats()) { assert!((a - b).abs() < 1e-6); }
    }

    #[test]
    fn hsv_known_values_and_alpha() {
        let hsv = eval(&Op::RgbToHsv, &[Value::Color4([1.0, 0.0, 0.0, 0.25])], Type::Color4);
        assert_eq!(hsv, Value::Color4([0.0, 1.0, 1.0, 0.25]));
        let rgb = eval(&Op::HsvToRgb, &[Value::Color3([0.5, 1.0, 1.0])], Type::Color3).floats();
        for (a, b) in rgb.iter().zip([0.0, 1.0, 1.0]) { assert!((a - b).abs() < 1e-6); }
    }

    #[test]
    fn luminance_keeps_alpha() {
        let coeffs = Value::Color3([0.25, 0.5, 0.25]);
        assert_eq!(eval(&Op::Luminance, &[Value::Color3([1.0, 0.0, 1.0]), coeffs.clone()], Type::Color3), Value::Color3([0.5, 0.5, 0.5]));
        assert_eq!(eval(&Op::Luminance, &[Value::Color4([1.0, 0.0, 1.0, 0.3]), coeffs], Type::Color4), Value::Color4([0.5, 0.5, 0.5, 0.3]));
    }
}
