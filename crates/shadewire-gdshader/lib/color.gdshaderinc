// Port of MaterialX libraries/stdlib/genglsl/lib/mx_hsv.glsl (Apache-2.0, see NOTICE).
// Derived from Open Shading Language opcolor.cpp, Copyright (c) 2009-2010 Sony Pictures Imageworks Inc.,
// et al., BSD-3-Clause (see NOTICE). Mirrors crates/shadewire/src/ops/color.rs.

// mx_hsvtorgb (Rust hsv_to_rgb)
vec3 shadewire_hsvtorgb(vec3 hsv) {
    float h = hsv.x;
    float s = hsv.y;
    float v = hsv.z;
    if (s < 0.0001) {
        return vec3(v, v, v);
    }
    h = 6.0 * (h - floor(h));
    int hi = int(trunc(h));
    float f = h - float(hi);
    float p = v * (1.0 - s);
    float q = v * (1.0 - s * f);
    float t = v * (1.0 - s * (1.0 - f));
    if (hi == 0) {
        return vec3(v, t, p);
    } else if (hi == 1) {
        return vec3(q, v, p);
    } else if (hi == 2) {
        return vec3(p, v, t);
    } else if (hi == 3) {
        return vec3(p, q, v);
    } else if (hi == 4) {
        return vec3(t, p, v);
    }
    return vec3(v, p, q);
}

// mx_rgbtohsv (Rust rgb_to_hsv)
vec3 shadewire_rgbtohsv(vec3 c) {
    float r = c.x;
    float g = c.y;
    float b = c.z;
    float mincomp = min(r, min(g, b));
    float maxcomp = max(r, max(g, b));
    float delta = maxcomp - mincomp;
    float v = maxcomp;
    float s = 0.0;
    if (maxcomp > 0.0) {
        s = delta / maxcomp;
    }
    float h = 0.0;
    if (s > 0.0) {
        if (r >= maxcomp) {
            h = (g - b) / delta;
        } else if (g >= maxcomp) {
            h = 2.0 + (b - r) / delta;
        } else {
            h = 4.0 + (r - g) / delta;
        }
        h *= 1.0 / 6.0;
        if (h < 0.0) {
            h += 1.0;
        }
    }
    return vec3(h, s, v);
}
