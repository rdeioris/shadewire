//! Worley (cellular) noise.
//! Ported from MaterialX libraries/stdlib/genglsl/lib/mx_noise.glsl (Apache-2.0), itself derived
//! from OSL noise, Copyright (c) 2009-2010 Sony Pictures Imageworks Inc., et al., BSD-3-Clause.
//! See NOTICE.

use super::basics::floorfrac;
use super::cell::{cell_noise_float_2d, cell_noise_float_3d, cell_noise_vec3_2d, cell_noise_vec3_3d};

// mx_worley_cell_position(int, int, int, int, float)
pub fn worley_cell_position_2d(x: i32, y: i32, xoff: i32, yoff: i32, jitter: f32) -> [f32; 2] {
    let tmp = cell_noise_vec3_2d([x.wrapping_add(xoff) as f32, y.wrapping_add(yoff) as f32]);
    let mut off = [tmp[0], tmp[1]];
    off = [off[0] - 0.5, off[1] - 0.5];
    off = [off[0] * jitter, off[1] * jitter];
    off = [off[0] + 0.5, off[1] + 0.5];
    [x as f32 + off[0], y as f32 + off[1]]
}

// mx_worley_cell_position(int, int, int, int, int, int, float)
pub fn worley_cell_position_3d(x: i32, y: i32, z: i32, xoff: i32, yoff: i32, zoff: i32, jitter: f32) -> [f32; 3] {
    let mut off = cell_noise_vec3_3d([
        x.wrapping_add(xoff) as f32,
        y.wrapping_add(yoff) as f32,
        z.wrapping_add(zoff) as f32,
    ]);
    off = [off[0] - 0.5, off[1] - 0.5, off[2] - 0.5];
    off = [off[0] * jitter, off[1] * jitter, off[2] * jitter];
    off = [off[0] + 0.5, off[1] + 0.5, off[2] + 0.5];
    [x as f32 + off[0], y as f32 + off[1], z as f32 + off[2]]
}

// mx_worley_distance(vec2, int, int, int, int, float, int)
pub fn worley_distance_2d(p: [f32; 2], x: i32, y: i32, xoff: i32, yoff: i32, jitter: f32, metric: i32) -> f32 {
    let cellpos = worley_cell_position_2d(x, y, xoff, yoff, jitter);
    let diff = [cellpos[0] - p[0], cellpos[1] - p[1]];
    if metric == 2 {
        return diff[0].abs() + diff[1].abs(); // Manhattan distance
    }
    if metric == 3 {
        return diff[0].abs().max(diff[1].abs()); // Chebyshev distance
    }
    // Either Euclidean or Distance^2
    diff[0] * diff[0] + diff[1] * diff[1]
}

// mx_worley_distance(vec3, int, int, int, int, int, int, float, int)
#[allow(clippy::too_many_arguments)]
pub fn worley_distance_3d(
    p: [f32; 3], x: i32, y: i32, z: i32, xoff: i32, yoff: i32, zoff: i32, jitter: f32, metric: i32,
) -> f32 {
    let cellpos = worley_cell_position_3d(x, y, z, xoff, yoff, zoff, jitter);
    let diff = [cellpos[0] - p[0], cellpos[1] - p[1], cellpos[2] - p[2]];
    if metric == 2 {
        return diff[0].abs() + diff[1].abs() + diff[2].abs(); // Manhattan distance
    }
    if metric == 3 {
        return diff[0].abs().max(diff[1].abs()).max(diff[2].abs()); // Chebyshev distance
    }
    // Either Euclidean or Distance^2
    diff[0] * diff[0] + diff[1] * diff[1] + diff[2] * diff[2]
}

// mx_worley_noise_float(vec2, float, int, int)
pub fn worley_noise_float_2d(p: [f32; 2], jitter: f32, style: i32, metric: i32) -> f32 {
    let (lx, xi) = floorfrac(p[0]);
    let (ly, yi) = floorfrac(p[1]);
    let localpos = [lx, ly];
    let mut sqdist = 1e6_f32; // Some big number for jitter > 1 (not all GPUs may be IEEE)
    let mut minpos = [0.0_f32, 0.0];
    for x in -1..=1 {
        for y in -1..=1 {
            let dist = worley_distance_2d(localpos, x, y, xi, yi, jitter, metric);
            let cp = worley_cell_position_2d(x, y, xi, yi, jitter);
            let cellpos = [cp[0] - localpos[0], cp[1] - localpos[1]];
            if dist < sqdist {
                sqdist = dist;
                minpos = cellpos;
            }
        }
    }
    if style == 1 {
        cell_noise_float_2d([minpos[0] + p[0], minpos[1] + p[1]])
    } else {
        if metric == 0 {
            sqdist = sqdist.sqrt();
        }
        sqdist
    }
}

// mx_worley_noise_vec2(vec2, float, int, int)
pub fn worley_noise_vec2_2d(p: [f32; 2], jitter: f32, style: i32, metric: i32) -> [f32; 2] {
    let (lx, xi) = floorfrac(p[0]);
    let (ly, yi) = floorfrac(p[1]);
    let localpos = [lx, ly];
    let mut sqdist = [1e6_f32, 1e6];
    let mut minpos = [0.0_f32, 0.0];
    for x in -1..=1 {
        for y in -1..=1 {
            let dist = worley_distance_2d(localpos, x, y, xi, yi, jitter, metric);
            let cp = worley_cell_position_2d(x, y, xi, yi, jitter);
            let cellpos = [cp[0] - localpos[0], cp[1] - localpos[1]];
            if dist < sqdist[0] {
                sqdist[1] = sqdist[0];
                sqdist[0] = dist;
                minpos = cellpos;
            } else if dist < sqdist[1] {
                sqdist[1] = dist;
            }
        }
    }
    if style == 1 {
        let tmp = cell_noise_vec3_2d([minpos[0] + p[0], minpos[1] + p[1]]);
        [tmp[0], tmp[1]]
    } else {
        if metric == 0 {
            sqdist = [sqdist[0].sqrt(), sqdist[1].sqrt()];
        }
        sqdist
    }
}

// mx_worley_noise_vec3(vec2, float, int, int)
pub fn worley_noise_vec3_2d(p: [f32; 2], jitter: f32, style: i32, metric: i32) -> [f32; 3] {
    let (lx, xi) = floorfrac(p[0]);
    let (ly, yi) = floorfrac(p[1]);
    let localpos = [lx, ly];
    let mut sqdist = [1e6_f32, 1e6, 1e6];
    let mut minpos = [0.0_f32, 0.0];
    for x in -1..=1 {
        for y in -1..=1 {
            let dist = worley_distance_2d(localpos, x, y, xi, yi, jitter, metric);
            let cp = worley_cell_position_2d(x, y, xi, yi, jitter);
            let cellpos = [cp[0] - localpos[0], cp[1] - localpos[1]];
            if dist < sqdist[0] {
                sqdist[2] = sqdist[1];
                sqdist[1] = sqdist[0];
                sqdist[0] = dist;
                minpos = cellpos;
            } else if dist < sqdist[1] {
                sqdist[2] = sqdist[1];
                sqdist[1] = dist;
            } else if dist < sqdist[2] {
                sqdist[2] = dist;
            }
        }
    }
    if style == 1 {
        cell_noise_vec3_2d([minpos[0] + p[0], minpos[1] + p[1]])
    } else {
        if metric == 0 {
            sqdist = [sqdist[0].sqrt(), sqdist[1].sqrt(), sqdist[2].sqrt()];
        }
        sqdist
    }
}

// mx_worley_noise_float(vec3, float, int, int)
pub fn worley_noise_float_3d(p: [f32; 3], jitter: f32, style: i32, metric: i32) -> f32 {
    let (lx, xi) = floorfrac(p[0]);
    let (ly, yi) = floorfrac(p[1]);
    let (lz, zi) = floorfrac(p[2]);
    let localpos = [lx, ly, lz];
    let mut sqdist = 1e6_f32;
    let mut minpos = [0.0_f32, 0.0, 0.0];
    for x in -1..=1 {
        for y in -1..=1 {
            for z in -1..=1 {
                let dist = worley_distance_3d(localpos, x, y, z, xi, yi, zi, jitter, metric);
                let cp = worley_cell_position_3d(x, y, z, xi, yi, zi, jitter);
                let cellpos = [cp[0] - localpos[0], cp[1] - localpos[1], cp[2] - localpos[2]];
                if dist < sqdist {
                    sqdist = dist;
                    minpos = cellpos;
                }
            }
        }
    }
    if style == 1 {
        cell_noise_float_3d([minpos[0] + p[0], minpos[1] + p[1], minpos[2] + p[2]])
    } else {
        if metric == 0 {
            sqdist = sqdist.sqrt();
        }
        sqdist
    }
}

// mx_worley_noise_vec2(vec3, float, int, int)
pub fn worley_noise_vec2_3d(p: [f32; 3], jitter: f32, style: i32, metric: i32) -> [f32; 2] {
    let (lx, xi) = floorfrac(p[0]);
    let (ly, yi) = floorfrac(p[1]);
    let (lz, zi) = floorfrac(p[2]);
    let localpos = [lx, ly, lz];
    let mut sqdist = [1e6_f32, 1e6];
    let mut minpos = [0.0_f32, 0.0, 0.0];
    for x in -1..=1 {
        for y in -1..=1 {
            for z in -1..=1 {
                let dist = worley_distance_3d(localpos, x, y, z, xi, yi, zi, jitter, metric);
                let cp = worley_cell_position_3d(x, y, z, xi, yi, zi, jitter);
                let cellpos = [cp[0] - localpos[0], cp[1] - localpos[1], cp[2] - localpos[2]];
                if dist < sqdist[0] {
                    sqdist[1] = sqdist[0];
                    sqdist[0] = dist;
                    minpos = cellpos;
                } else if dist < sqdist[1] {
                    sqdist[1] = dist;
                }
            }
        }
    }
    if style == 1 {
        let tmp = cell_noise_vec3_3d([minpos[0] + p[0], minpos[1] + p[1], minpos[2] + p[2]]);
        [tmp[0], tmp[1]]
    } else {
        if metric == 0 {
            sqdist = [sqdist[0].sqrt(), sqdist[1].sqrt()];
        }
        sqdist
    }
}

// mx_worley_noise_vec3(vec3, float, int, int)
pub fn worley_noise_vec3_3d(p: [f32; 3], jitter: f32, style: i32, metric: i32) -> [f32; 3] {
    let (lx, xi) = floorfrac(p[0]);
    let (ly, yi) = floorfrac(p[1]);
    let (lz, zi) = floorfrac(p[2]);
    let localpos = [lx, ly, lz];
    let mut sqdist = [1e6_f32, 1e6, 1e6];
    let mut minpos = [0.0_f32, 0.0, 0.0];
    for x in -1..=1 {
        for y in -1..=1 {
            for z in -1..=1 {
                let dist = worley_distance_3d(localpos, x, y, z, xi, yi, zi, jitter, metric);
                let cp = worley_cell_position_3d(x, y, z, xi, yi, zi, jitter);
                let cellpos = [cp[0] - localpos[0], cp[1] - localpos[1], cp[2] - localpos[2]];
                if dist < sqdist[0] {
                    sqdist[2] = sqdist[1];
                    sqdist[1] = sqdist[0];
                    sqdist[0] = dist;
                    minpos = cellpos;
                } else if dist < sqdist[1] {
                    sqdist[2] = sqdist[1];
                    sqdist[1] = dist;
                } else if dist < sqdist[2] {
                    sqdist[2] = dist;
                }
            }
        }
    }
    if style == 1 {
        cell_noise_vec3_3d([minpos[0] + p[0], minpos[1] + p[1], minpos[2] + p[2]])
    } else {
        if metric == 0 {
            sqdist = [sqdist[0].sqrt(), sqdist[1].sqrt(), sqdist[2].sqrt()];
        }
        sqdist
    }
}
