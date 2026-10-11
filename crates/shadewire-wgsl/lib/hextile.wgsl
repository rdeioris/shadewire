// Port of MaterialX libraries/stdlib/genglsl/lib/mx_hextile.glsl (Apache-2.0, see NOTICE).
// Mirrors crates/shadewire-ir/src/ops/hextile.rs: tiles are laid out in the V-down space
// MaterialX's GLSL uses, so each function flips its coordinate in and its result out.
// Morten S. Mikkelsen, Practical Real-Time Hex-Tiling, JCGT 11(2), 2022.
// Precision limit: the float hash moves by about 1e-3 when a driver fuses its multiply-adds
// (about 0.4 degrees of tile rotation, 1e-3 uv of offset); MaterialX's GLSL varies the same way.

const MX_HEXTILE_SKEW_X: f32 = -0.57735027;
const MX_HEXTILE_SKEW_Y: f32 = 1.15470054;

fn mx_hextile_sqrt3_2() -> f32 {
    return sqrt(3.0) * 2.0;
}

fn mx_hextile_mix(a: f32, b: f32, t: f32) -> f32 {
    return a * (1.0 - t) + b * t;
}

fn mx_hextile_flip(p: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(p.x, 1.0 - p.y);
}

struct MxHextileLayout {
    id1: vec2<f32>,
    id2: vec2<f32>,
    id3: vec2<f32>,
    weights: vec3<f32>,
}

fn mx_hextile_layout(coord: vec2<f32>) -> MxHextileLayout {
    let st = coord * mx_hextile_sqrt3_2();
    let skewed = vec2<f32>(st.x + MX_HEXTILE_SKEW_X * st.y, MX_HEXTILE_SKEW_Y * st.y);
    let fx = skewed.x - floor(skewed.x);
    let fy = skewed.y - floor(skewed.y);
    let z = 1.0 - fx - fy;
    let s = select(1.0, 0.0, -z < 0.0);
    let s2 = 2.0 * s - 1.0;
    let base = floor(skewed);
    return MxHextileLayout(
        vec2<f32>(base.x + s, base.y + s),
        vec2<f32>(base.x + s, base.y + 1.0 - s),
        vec2<f32>(base.x + 1.0 - s, base.y + s),
        vec3<f32>(-z * s2, s - fy * s2, s - fx * s2),
    );
}

fn mx_hextile_id(coord: vec2<f32>, tile: i32) -> vec2<f32> {
    let l = mx_hextile_layout(coord);
    return select(select(l.id3, l.id2, tile == 1), l.id1, tile == 0);
}

fn mx_hextile_center(id: vec2<f32>) -> vec2<f32> {
    let p = id / mx_hextile_sqrt3_2();
    return vec2<f32>(p.x + 0.5 * p.y, (1.0 / MX_HEXTILE_SKEW_Y) * p.y);
}

// https://www.shadertoy.com/view/4djSRW, on id + 0.12345.
fn mx_hextile_hash(id: vec2<f32>) -> vec2<f32> {
    let p = id + 0.12345;
    let q = vec3<f32>(p.x * 0.1031, p.y * 0.1030, p.x * 0.0973);
    var p3 = q - floor(q);
    let d = p3.x * (p3.y + 33.33) + p3.y * (p3.z + 33.33) + p3.z * (p3.x + 33.33);
    p3 = p3 + d;
    let r = vec2<f32>((p3.x + p3.y) * p3.z, (p3.x + p3.z) * p3.y);
    return r - floor(r);
}

// The tile's random rotation as (sin, cos) and its scale.
fn mx_hextile_transform(rand: vec2<f32>, rotation: f32, rotation_range: vec2<f32>, scale: f32, scale_range: vec2<f32>) -> vec3<f32> {
    let to_radians = 3.14159265358979323846 / 180.0;
    let angle = mx_hextile_mix(rotation_range.x * to_radians, rotation_range.y * to_radians, rand.x * rotation);
    let s = mx_hextile_mix(1.0, mx_hextile_mix(scale_range.x, scale_range.y, rand.y), scale);
    return vec3<f32>(sin(angle), cos(angle), s);
}

// mx_matrix_mul(v, mat2(c, -s, s, c)) / scale.
fn mx_hextile_rotate(v: vec2<f32>, t: vec3<f32>) -> vec2<f32> {
    return vec2<f32>((t.y * v.x - t.x * v.y) / t.z, (t.x * v.x + t.y * v.y) / t.z);
}

fn mx_hextile_coord(coord: vec2<f32>, tile: i32, rotation: f32, rotation_range: vec2<f32>, scale: f32, scale_range: vec2<f32>, offset: f32, offset_range: vec2<f32>) -> vec2<f32> {
    let c = mx_hextile_flip(coord);
    let id = mx_hextile_id(c, tile);
    let rand = mx_hextile_hash(id);
    let t = mx_hextile_transform(rand, rotation, rotation_range, scale, scale_range);
    let shift = vec2<f32>(
        mx_hextile_mix(offset_range.x, offset_range.y, rand.x * offset),
        mx_hextile_mix(offset_range.x, offset_range.y, rand.y * offset),
    );
    let ctr = mx_hextile_center(id);
    let r = mx_hextile_rotate(c - ctr, t);
    return mx_hextile_flip(r + ctr + shift);
}

fn mx_hextile_grad(grad: vec2<f32>, coord: vec2<f32>, tile: i32, rotation: f32, rotation_range: vec2<f32>, scale: f32, scale_range: vec2<f32>) -> vec2<f32> {
    let id = mx_hextile_id(mx_hextile_flip(coord), tile);
    let t = mx_hextile_transform(mx_hextile_hash(id), rotation, rotation_range, scale, scale_range);
    let r = mx_hextile_rotate(vec2<f32>(grad.x, -grad.y), t);
    return vec2<f32>(r.x, -r.y);
}

// mx_schlick_gain: Christophe Schlick, Graphics Gems IV.
fn mx_hextile_gain(x: f32, r: f32) -> f32 {
    let rr = clamp(r, 0.001, 0.999);
    let a = (1.0 / rr - 2.0) * (1.0 - 2.0 * x);
    return select((a - x) / (a - 1.0), x / (a + 1.0), x < 0.5);
}

// mx_hextile_compute_blend_weights over the coordinate's barycentric tile weights.
fn mx_hextile_weights(coord: vec2<f32>, luminance: vec3<f32>, falloff: f32) -> vec3<f32> {
    let tile = mx_hextile_layout(mx_hextile_flip(coord)).weights;
    var w = luminance * pow(tile, vec3<f32>(7.0));
    w = w / (w.x + w.y + w.z);
    if falloff != 0.5 {
        w = vec3<f32>(mx_hextile_gain(w.x, falloff), mx_hextile_gain(w.y, falloff), mx_hextile_gain(w.z, falloff));
        w = w / (w.x + w.y + w.z);
    }
    return w;
}
