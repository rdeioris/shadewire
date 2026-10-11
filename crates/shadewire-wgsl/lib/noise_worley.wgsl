// Port of MaterialX libraries/stdlib/genglsl/lib/mx_noise.glsl (Apache-2.0, see NOTICE).
// Derived from Open Shading Language oslnoise.h, Copyright (c) 2009-2010 Sony Pictures Imageworks Inc.,
// et al., BSD-3-Clause (see NOTICE). Mirrors crates/shadewire-ir/src/ops/noise/worley.rs.
// Needs noise_basics, noise_hash and noise_cell. WGSL i32 `+` wraps like Rust's wrapping_add.

// mx_worley_cell_position(int, int, int, int, float) (Rust worley_cell_position_2d)
fn mx_worley_cell_position_2d(x: i32, y: i32, xoff: i32, yoff: i32, jitter: f32) -> vec2<f32> {
    let tmp = mx_cell_noise_vec3_2d(vec2<f32>(f32(x + xoff), f32(y + yoff)));
    var off = vec2<f32>(tmp.x, tmp.y);
    off -= 0.5;
    off *= jitter;
    off += 0.5;
    return vec2<f32>(f32(x), f32(y)) + off;
}

// mx_worley_cell_position(int, int, int, int, int, int, float) (Rust worley_cell_position_3d)
fn mx_worley_cell_position_3d(x: i32, y: i32, z: i32, xoff: i32, yoff: i32, zoff: i32, jitter: f32) -> vec3<f32> {
    var off = mx_cell_noise_vec3_3d(vec3<f32>(f32(x + xoff), f32(y + yoff), f32(z + zoff)));
    off -= 0.5;
    off *= jitter;
    off += 0.5;
    return vec3<f32>(f32(x), f32(y), f32(z)) + off;
}

// mx_worley_distance(vec2, int, int, int, int, float, int) (Rust worley_distance_2d)
fn mx_worley_distance_2d(p: vec2<f32>, x: i32, y: i32, xoff: i32, yoff: i32, jitter: f32, metric: i32) -> f32 {
    let cellpos = mx_worley_cell_position_2d(x, y, xoff, yoff, jitter);
    let diff = cellpos - p;
    if metric == 2 {
        return abs(diff.x) + abs(diff.y); // Manhattan distance
    }
    if metric == 3 {
        return max(abs(diff.x), abs(diff.y)); // Chebyshev distance
    }
    // Either Euclidean or Distance^2
    return diff.x * diff.x + diff.y * diff.y;
}

// mx_worley_distance(vec3, int, int, int, int, int, int, float, int) (Rust worley_distance_3d)
fn mx_worley_distance_3d(p: vec3<f32>, x: i32, y: i32, z: i32, xoff: i32, yoff: i32, zoff: i32, jitter: f32, metric: i32) -> f32 {
    let cellpos = mx_worley_cell_position_3d(x, y, z, xoff, yoff, zoff, jitter);
    let diff = cellpos - p;
    if metric == 2 {
        return abs(diff.x) + abs(diff.y) + abs(diff.z); // Manhattan distance
    }
    if metric == 3 {
        return max(max(abs(diff.x), abs(diff.y)), abs(diff.z)); // Chebyshev distance
    }
    // Either Euclidean or Distance^2
    return diff.x * diff.x + diff.y * diff.y + diff.z * diff.z;
}

// mx_worley_noise_float(vec2, float, int, int) (Rust worley_noise_float_2d)
fn mx_worley_noise_float_2d(p: vec2<f32>, jitter: f32, style: i32, metric: i32) -> f32 {
    var xi: i32;
    var yi: i32;
    let localpos = vec2<f32>(mx_floorfrac(p.x, &xi), mx_floorfrac(p.y, &yi));
    var sqdist = 1e6; // Some big number for jitter > 1 (not all GPUs may be IEEE)
    var minpos = vec2<f32>(0.0, 0.0);
    for (var x = -1; x <= 1; x++) {
        for (var y = -1; y <= 1; y++) {
            let dist = mx_worley_distance_2d(localpos, x, y, xi, yi, jitter, metric);
            let cellpos = mx_worley_cell_position_2d(x, y, xi, yi, jitter) - localpos;
            if dist < sqdist {
                sqdist = dist;
                minpos = cellpos;
            }
        }
    }
    if style == 1 {
        return mx_cell_noise_float_2d(minpos + p);
    }
    if metric == 0 {
        sqdist = sqrt(sqdist);
    }
    return sqdist;
}

// mx_worley_noise_vec2(vec2, float, int, int) (Rust worley_noise_vec2_2d)
fn mx_worley_noise_vec2_2d(p: vec2<f32>, jitter: f32, style: i32, metric: i32) -> vec2<f32> {
    var xi: i32;
    var yi: i32;
    let localpos = vec2<f32>(mx_floorfrac(p.x, &xi), mx_floorfrac(p.y, &yi));
    var sqdist = vec2<f32>(1e6, 1e6);
    var minpos = vec2<f32>(0.0, 0.0);
    for (var x = -1; x <= 1; x++) {
        for (var y = -1; y <= 1; y++) {
            let dist = mx_worley_distance_2d(localpos, x, y, xi, yi, jitter, metric);
            let cellpos = mx_worley_cell_position_2d(x, y, xi, yi, jitter) - localpos;
            if dist < sqdist.x {
                sqdist.y = sqdist.x;
                sqdist.x = dist;
                minpos = cellpos;
            } else if dist < sqdist.y {
                sqdist.y = dist;
            }
        }
    }
    if style == 1 {
        let tmp = mx_cell_noise_vec3_2d(minpos + p);
        return vec2<f32>(tmp.x, tmp.y);
    }
    if metric == 0 {
        sqdist = sqrt(sqdist);
    }
    return sqdist;
}

// mx_worley_noise_vec3(vec2, float, int, int) (Rust worley_noise_vec3_2d)
fn mx_worley_noise_vec3_2d(p: vec2<f32>, jitter: f32, style: i32, metric: i32) -> vec3<f32> {
    var xi: i32;
    var yi: i32;
    let localpos = vec2<f32>(mx_floorfrac(p.x, &xi), mx_floorfrac(p.y, &yi));
    var sqdist = vec3<f32>(1e6, 1e6, 1e6);
    var minpos = vec2<f32>(0.0, 0.0);
    for (var x = -1; x <= 1; x++) {
        for (var y = -1; y <= 1; y++) {
            let dist = mx_worley_distance_2d(localpos, x, y, xi, yi, jitter, metric);
            let cellpos = mx_worley_cell_position_2d(x, y, xi, yi, jitter) - localpos;
            if dist < sqdist.x {
                sqdist.z = sqdist.y;
                sqdist.y = sqdist.x;
                sqdist.x = dist;
                minpos = cellpos;
            } else if dist < sqdist.y {
                sqdist.z = sqdist.y;
                sqdist.y = dist;
            } else if dist < sqdist.z {
                sqdist.z = dist;
            }
        }
    }
    if style == 1 {
        return mx_cell_noise_vec3_2d(minpos + p);
    }
    if metric == 0 {
        sqdist = sqrt(sqdist);
    }
    return sqdist;
}

// mx_worley_noise_float(vec3, float, int, int) (Rust worley_noise_float_3d)
fn mx_worley_noise_float_3d(p: vec3<f32>, jitter: f32, style: i32, metric: i32) -> f32 {
    var xi: i32;
    var yi: i32;
    var zi: i32;
    let localpos = vec3<f32>(mx_floorfrac(p.x, &xi), mx_floorfrac(p.y, &yi), mx_floorfrac(p.z, &zi));
    var sqdist = 1e6;
    var minpos = vec3<f32>(0.0, 0.0, 0.0);
    for (var x = -1; x <= 1; x++) {
        for (var y = -1; y <= 1; y++) {
            for (var z = -1; z <= 1; z++) {
                let dist = mx_worley_distance_3d(localpos, x, y, z, xi, yi, zi, jitter, metric);
                let cellpos = mx_worley_cell_position_3d(x, y, z, xi, yi, zi, jitter) - localpos;
                if dist < sqdist {
                    sqdist = dist;
                    minpos = cellpos;
                }
            }
        }
    }
    if style == 1 {
        return mx_cell_noise_float_3d(minpos + p);
    }
    if metric == 0 {
        sqdist = sqrt(sqdist);
    }
    return sqdist;
}

// mx_worley_noise_vec2(vec3, float, int, int) (Rust worley_noise_vec2_3d)
fn mx_worley_noise_vec2_3d(p: vec3<f32>, jitter: f32, style: i32, metric: i32) -> vec2<f32> {
    var xi: i32;
    var yi: i32;
    var zi: i32;
    let localpos = vec3<f32>(mx_floorfrac(p.x, &xi), mx_floorfrac(p.y, &yi), mx_floorfrac(p.z, &zi));
    var sqdist = vec2<f32>(1e6, 1e6);
    var minpos = vec3<f32>(0.0, 0.0, 0.0);
    for (var x = -1; x <= 1; x++) {
        for (var y = -1; y <= 1; y++) {
            for (var z = -1; z <= 1; z++) {
                let dist = mx_worley_distance_3d(localpos, x, y, z, xi, yi, zi, jitter, metric);
                let cellpos = mx_worley_cell_position_3d(x, y, z, xi, yi, zi, jitter) - localpos;
                if dist < sqdist.x {
                    sqdist.y = sqdist.x;
                    sqdist.x = dist;
                    minpos = cellpos;
                } else if dist < sqdist.y {
                    sqdist.y = dist;
                }
            }
        }
    }
    if style == 1 {
        let tmp = mx_cell_noise_vec3_3d(minpos + p);
        return vec2<f32>(tmp.x, tmp.y);
    }
    if metric == 0 {
        sqdist = sqrt(sqdist);
    }
    return sqdist;
}

// mx_worley_noise_vec3(vec3, float, int, int) (Rust worley_noise_vec3_3d)
fn mx_worley_noise_vec3_3d(p: vec3<f32>, jitter: f32, style: i32, metric: i32) -> vec3<f32> {
    var xi: i32;
    var yi: i32;
    var zi: i32;
    let localpos = vec3<f32>(mx_floorfrac(p.x, &xi), mx_floorfrac(p.y, &yi), mx_floorfrac(p.z, &zi));
    var sqdist = vec3<f32>(1e6, 1e6, 1e6);
    var minpos = vec3<f32>(0.0, 0.0, 0.0);
    for (var x = -1; x <= 1; x++) {
        for (var y = -1; y <= 1; y++) {
            for (var z = -1; z <= 1; z++) {
                let dist = mx_worley_distance_3d(localpos, x, y, z, xi, yi, zi, jitter, metric);
                let cellpos = mx_worley_cell_position_3d(x, y, z, xi, yi, zi, jitter) - localpos;
                if dist < sqdist.x {
                    sqdist.z = sqdist.y;
                    sqdist.y = sqdist.x;
                    sqdist.x = dist;
                    minpos = cellpos;
                } else if dist < sqdist.y {
                    sqdist.z = sqdist.y;
                    sqdist.y = dist;
                } else if dist < sqdist.z {
                    sqdist.z = dist;
                }
            }
        }
    }
    if style == 1 {
        return mx_cell_noise_vec3_3d(minpos + p);
    }
    if metric == 0 {
        sqdist = sqrt(sqdist);
    }
    return sqdist;
}
