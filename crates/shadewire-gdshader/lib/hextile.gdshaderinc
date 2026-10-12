// Port of MaterialX libraries/stdlib/genglsl/lib/mx_hextile.glsl (Apache-2.0, see NOTICE).
// Mirrors crates/shadewire/src/ops/hextile.rs: tiles are laid out in the V-down space
// MaterialX's GLSL uses, so each function flips its coordinate in and its result out.
// Morten S. Mikkelsen, Practical Real-Time Hex-Tiling, JCGT 11(2), 2022.
// Precision limit: the float hash moves by about 1e-3 when a driver fuses its multiply-adds
// (about 0.4 degrees of tile rotation, 1e-3 uv of offset); MaterialX's GLSL varies the same way.

const float MX_HEXTILE_SKEW_X = -0.57735027;
const float MX_HEXTILE_SKEW_Y = 1.15470054;

struct MxHextileLayout {
    vec2 id1;
    vec2 id2;
    vec2 id3;
    vec3 weights;
};

float shadewire_hextile_sqrt3_2() {
    return sqrt(3.0) * 2.0;
}

float shadewire_hextile_mix(float a, float b, float t) {
    return a * (1.0 - t) + b * t;
}

vec2 shadewire_hextile_flip(vec2 p) {
    return vec2(p.x, 1.0 - p.y);
}

MxHextileLayout shadewire_hextile_layout(vec2 coord) {
    vec2 st = coord * shadewire_hextile_sqrt3_2();
    vec2 skewed = vec2(st.x + MX_HEXTILE_SKEW_X * st.y, MX_HEXTILE_SKEW_Y * st.y);
    float fx = skewed.x - floor(skewed.x);
    float fy = skewed.y - floor(skewed.y);
    float z = 1.0 - fx - fy;
    float s = (-z < 0.0) ? 0.0 : 1.0;
    float s2 = 2.0 * s - 1.0;
    vec2 cell = floor(skewed);
    return MxHextileLayout(
        vec2(cell.x + s, cell.y + s),
        vec2(cell.x + s, cell.y + 1.0 - s),
        vec2(cell.x + 1.0 - s, cell.y + s),
        vec3(-z * s2, s - fy * s2, s - fx * s2));
}

vec2 shadewire_hextile_id(vec2 coord, int tile) {
    MxHextileLayout l = shadewire_hextile_layout(coord);
    return (tile == 0) ? l.id1 : ((tile == 1) ? l.id2 : l.id3);
}

vec2 shadewire_hextile_center(vec2 id) {
    vec2 p = id / shadewire_hextile_sqrt3_2();
    return vec2(p.x + 0.5 * p.y, (1.0 / MX_HEXTILE_SKEW_Y) * p.y);
}

// https://www.shadertoy.com/view/4djSRW, on id + 0.12345.
vec2 shadewire_hextile_hash(vec2 id) {
    vec2 p = id + 0.12345;
    vec3 q = vec3(p.x * 0.1031, p.y * 0.1030, p.x * 0.0973);
    vec3 p3 = q - floor(q);
    float d = p3.x * (p3.y + 33.33) + p3.y * (p3.z + 33.33) + p3.z * (p3.x + 33.33);
    p3 = p3 + d;
    vec2 r = vec2((p3.x + p3.y) * p3.z, (p3.x + p3.z) * p3.y);
    return r - floor(r);
}

// The tile's random rotation as (sin, cos) and its scale.
vec3 shadewire_hextile_transform(vec2 rand, float rot, vec2 rot_range, float scl, vec2 scl_range) {
    float to_radians = 3.14159265358979323846 / 180.0;
    float angle = shadewire_hextile_mix(rot_range.x * to_radians, rot_range.y * to_radians, rand.x * rot);
    float k = shadewire_hextile_mix(1.0, shadewire_hextile_mix(scl_range.x, scl_range.y, rand.y), scl);
    return vec3(sin(angle), cos(angle), k);
}

// mx_matrix_mul(v, mat2(c, -s, s, c)) / scale.
vec2 shadewire_hextile_rotate(vec2 v, vec3 t) {
    return vec2((t.y * v.x - t.x * v.y) / t.z, (t.x * v.x + t.y * v.y) / t.z);
}

vec2 shadewire_hextile_coord(vec2 coord, int tile, float rot, vec2 rot_range, float scl, vec2 scl_range, float ofs, vec2 ofs_range) {
    vec2 c = shadewire_hextile_flip(coord);
    vec2 id = shadewire_hextile_id(c, tile);
    vec2 rand = shadewire_hextile_hash(id);
    vec3 t = shadewire_hextile_transform(rand, rot, rot_range, scl, scl_range);
    vec2 shift = vec2(
        shadewire_hextile_mix(ofs_range.x, ofs_range.y, rand.x * ofs),
        shadewire_hextile_mix(ofs_range.x, ofs_range.y, rand.y * ofs));
    vec2 ctr = shadewire_hextile_center(id);
    vec2 r = shadewire_hextile_rotate(c - ctr, t);
    return shadewire_hextile_flip(r + ctr + shift);
}

vec2 shadewire_hextile_grad(vec2 grad, vec2 coord, int tile, float rot, vec2 rot_range, float scl, vec2 scl_range) {
    vec2 id = shadewire_hextile_id(shadewire_hextile_flip(coord), tile);
    vec3 t = shadewire_hextile_transform(shadewire_hextile_hash(id), rot, rot_range, scl, scl_range);
    vec2 r = shadewire_hextile_rotate(vec2(grad.x, -grad.y), t);
    return vec2(r.x, -r.y);
}

// mx_schlick_gain: Christophe Schlick, Graphics Gems IV.
float shadewire_hextile_gain(float x, float r) {
    float rr = clamp(r, 0.001, 0.999);
    float a = (1.0 / rr - 2.0) * (1.0 - 2.0 * x);
    return (x < 0.5) ? x / (a + 1.0) : (a - x) / (a - 1.0);
}

// mx_hextile_compute_blend_weights over the coordinate's barycentric tile weights.
vec3 shadewire_hextile_weights(vec2 coord, vec3 lum, float fall) {
    vec3 tile = shadewire_hextile_layout(shadewire_hextile_flip(coord)).weights;
    vec3 w = lum * pow(tile, vec3(7.0));
    w = w / (w.x + w.y + w.z);
    if (fall != 0.5) {
        w = vec3(shadewire_hextile_gain(w.x, fall), shadewire_hextile_gain(w.y, fall), shadewire_hextile_gain(w.z, fall));
        w = w / (w.x + w.y + w.z);
    }
    return w;
}
