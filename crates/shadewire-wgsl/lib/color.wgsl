// Port of MaterialX libraries/stdlib/genglsl/lib/mx_hsv.glsl (Apache-2.0, see NOTICE).
// Derived from Open Shading Language opcolor.cpp, Copyright (c) 2009-2010 Sony Pictures Imageworks Inc.,
// et al., BSD-3-Clause (see NOTICE). Mirrors crates/shadewire-ir/src/ops/color.rs.

// mx_hsvtorgb (Rust hsv_to_rgb)
fn mx_hsvtorgb(hsv: vec3<f32>) -> vec3<f32> {
    var h = hsv.x;
    let s = hsv.y;
    let v = hsv.z;
    if s < 0.0001 {
        return vec3<f32>(v, v, v);
    }
    h = 6.0 * (h - floor(h));
    let hi = i32(trunc(h));
    let f = h - f32(hi);
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    switch hi {
        case 0: { return vec3<f32>(v, t, p); }
        case 1: { return vec3<f32>(q, v, p); }
        case 2: { return vec3<f32>(p, v, t); }
        case 3: { return vec3<f32>(p, q, v); }
        case 4: { return vec3<f32>(t, p, v); }
        default: { return vec3<f32>(v, p, q); }
    }
}

// mx_rgbtohsv (Rust rgb_to_hsv)
fn mx_rgbtohsv(c: vec3<f32>) -> vec3<f32> {
    let r = c.x;
    let g = c.y;
    let b = c.z;
    let mincomp = min(r, min(g, b));
    let maxcomp = max(r, max(g, b));
    let delta = maxcomp - mincomp;
    let v = maxcomp;
    var s = 0.0;
    if maxcomp > 0.0 {
        s = delta / maxcomp;
    }
    var h: f32;
    if s <= 0.0 {
        h = 0.0;
    } else {
        if r >= maxcomp {
            h = (g - b) / delta;
        } else if g >= maxcomp {
            h = 2.0 + (b - r) / delta;
        } else {
            h = 4.0 + (r - g) / delta;
        }
        h *= 1.0 / 6.0;
        if h < 0.0 {
            h += 1.0;
        }
    }
    return vec3<f32>(h, s, v);
}
