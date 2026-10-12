// Port of MaterialX libraries/stdlib/genglsl/lib/mx_noise.glsl (Apache-2.0, see NOTICE).
// Derived from Open Shading Language oslnoise.h, Copyright (c) 2009-2010 Sony Pictures Imageworks Inc.,
// et al., BSD-3-Clause (see NOTICE). Mirrors crates/shadewire/src/ops/noise/worley.rs.
// Needs noise_basics, noise_hash and noise_cell. int `+` wraps like Rust's wrapping_add.

// mx_worley_cell_position(int, int, int, int, float) (Rust worley_cell_position_2d)
vec2 shadewire_worley_cell_position_2d(int x, int y, int xoff, int yoff, float jit) {
    vec3 tmp = shadewire_cell_noise_vec3_2d(vec2(float(x + xoff), float(y + yoff)));
    vec2 off = vec2(tmp.x, tmp.y);
    off -= 0.5;
    off *= jit;
    off += 0.5;
    return vec2(float(x), float(y)) + off;
}

// mx_worley_cell_position(int, int, int, int, int, int, float) (Rust worley_cell_position_3d)
vec3 shadewire_worley_cell_position_3d(int x, int y, int z, int xoff, int yoff, int zoff, float jit) {
    vec3 off = shadewire_cell_noise_vec3_3d(vec3(float(x + xoff), float(y + yoff), float(z + zoff)));
    off -= 0.5;
    off *= jit;
    off += 0.5;
    return vec3(float(x), float(y), float(z)) + off;
}

// mx_worley_distance(vec2, int, int, int, int, float, int) (Rust worley_distance_2d)
float shadewire_worley_distance_2d(vec2 p, int x, int y, int xoff, int yoff, float jit, int met) {
    vec2 cellpos = shadewire_worley_cell_position_2d(x, y, xoff, yoff, jit);
    vec2 diff = cellpos - p;
    if (met == 2) {
        return abs(diff.x) + abs(diff.y); // Manhattan distance
    }
    if (met == 3) {
        return max(abs(diff.x), abs(diff.y)); // Chebyshev distance
    }
    // Either Euclidean or Distance^2
    return diff.x * diff.x + diff.y * diff.y;
}

// mx_worley_distance(vec3, int, int, int, int, int, int, float, int) (Rust worley_distance_3d)
float shadewire_worley_distance_3d(vec3 p, int x, int y, int z, int xoff, int yoff, int zoff, float jit, int met) {
    vec3 cellpos = shadewire_worley_cell_position_3d(x, y, z, xoff, yoff, zoff, jit);
    vec3 diff = cellpos - p;
    if (met == 2) {
        return abs(diff.x) + abs(diff.y) + abs(diff.z); // Manhattan distance
    }
    if (met == 3) {
        return max(max(abs(diff.x), abs(diff.y)), abs(diff.z)); // Chebyshev distance
    }
    // Either Euclidean or Distance^2
    return diff.x * diff.x + diff.y * diff.y + diff.z * diff.z;
}

// mx_worley_noise_float(vec2, float, int, int) (Rust worley_noise_float_2d)
float shadewire_worley_noise_float_2d(vec2 p, float jit, int sty, int met) {
    int xi = 0;
    int yi = 0;
    vec2 localpos = vec2(shadewire_floorfrac(p.x, xi), shadewire_floorfrac(p.y, yi));
    float sqdist = 1000000.0; // Some big number for jitter > 1 (not all GPUs may be IEEE)
    vec2 minpos = vec2(0.0, 0.0);
    for (int x = -1; x <= 1; x++) {
        for (int y = -1; y <= 1; y++) {
            float dist = shadewire_worley_distance_2d(localpos, x, y, xi, yi, jit, met);
            vec2 cellpos = shadewire_worley_cell_position_2d(x, y, xi, yi, jit) - localpos;
            if (dist < sqdist) {
                sqdist = dist;
                minpos = cellpos;
            }
        }
    }
    if (sty == 1) {
        return shadewire_cell_noise_float_2d(minpos + p);
    }
    if (met == 0) {
        sqdist = sqrt(sqdist);
    }
    return sqdist;
}

// mx_worley_noise_vec2(vec2, float, int, int) (Rust worley_noise_vec2_2d)
vec2 shadewire_worley_noise_vec2_2d(vec2 p, float jit, int sty, int met) {
    int xi = 0;
    int yi = 0;
    vec2 localpos = vec2(shadewire_floorfrac(p.x, xi), shadewire_floorfrac(p.y, yi));
    vec2 sqdist = vec2(1000000.0, 1000000.0);
    vec2 minpos = vec2(0.0, 0.0);
    for (int x = -1; x <= 1; x++) {
        for (int y = -1; y <= 1; y++) {
            float dist = shadewire_worley_distance_2d(localpos, x, y, xi, yi, jit, met);
            vec2 cellpos = shadewire_worley_cell_position_2d(x, y, xi, yi, jit) - localpos;
            if (dist < sqdist.x) {
                sqdist.y = sqdist.x;
                sqdist.x = dist;
                minpos = cellpos;
            } else if (dist < sqdist.y) {
                sqdist.y = dist;
            }
        }
    }
    if (sty == 1) {
        vec3 tmp = shadewire_cell_noise_vec3_2d(minpos + p);
        return vec2(tmp.x, tmp.y);
    }
    if (met == 0) {
        sqdist = sqrt(sqdist);
    }
    return sqdist;
}

// mx_worley_noise_vec3(vec2, float, int, int) (Rust worley_noise_vec3_2d)
vec3 shadewire_worley_noise_vec3_2d(vec2 p, float jit, int sty, int met) {
    int xi = 0;
    int yi = 0;
    vec2 localpos = vec2(shadewire_floorfrac(p.x, xi), shadewire_floorfrac(p.y, yi));
    vec3 sqdist = vec3(1000000.0, 1000000.0, 1000000.0);
    vec2 minpos = vec2(0.0, 0.0);
    for (int x = -1; x <= 1; x++) {
        for (int y = -1; y <= 1; y++) {
            float dist = shadewire_worley_distance_2d(localpos, x, y, xi, yi, jit, met);
            vec2 cellpos = shadewire_worley_cell_position_2d(x, y, xi, yi, jit) - localpos;
            if (dist < sqdist.x) {
                sqdist.z = sqdist.y;
                sqdist.y = sqdist.x;
                sqdist.x = dist;
                minpos = cellpos;
            } else if (dist < sqdist.y) {
                sqdist.z = sqdist.y;
                sqdist.y = dist;
            } else if (dist < sqdist.z) {
                sqdist.z = dist;
            }
        }
    }
    if (sty == 1) {
        return shadewire_cell_noise_vec3_2d(minpos + p);
    }
    if (met == 0) {
        sqdist = sqrt(sqdist);
    }
    return sqdist;
}

// mx_worley_noise_float(vec3, float, int, int) (Rust worley_noise_float_3d)
float shadewire_worley_noise_float_3d(vec3 p, float jit, int sty, int met) {
    int xi = 0;
    int yi = 0;
    int zi = 0;
    vec3 localpos = vec3(shadewire_floorfrac(p.x, xi), shadewire_floorfrac(p.y, yi), shadewire_floorfrac(p.z, zi));
    float sqdist = 1000000.0;
    vec3 minpos = vec3(0.0, 0.0, 0.0);
    for (int x = -1; x <= 1; x++) {
        for (int y = -1; y <= 1; y++) {
            for (int z = -1; z <= 1; z++) {
                float dist = shadewire_worley_distance_3d(localpos, x, y, z, xi, yi, zi, jit, met);
                vec3 cellpos = shadewire_worley_cell_position_3d(x, y, z, xi, yi, zi, jit) - localpos;
                if (dist < sqdist) {
                    sqdist = dist;
                    minpos = cellpos;
                }
            }
        }
    }
    if (sty == 1) {
        return shadewire_cell_noise_float_3d(minpos + p);
    }
    if (met == 0) {
        sqdist = sqrt(sqdist);
    }
    return sqdist;
}

// mx_worley_noise_vec2(vec3, float, int, int) (Rust worley_noise_vec2_3d)
vec2 shadewire_worley_noise_vec2_3d(vec3 p, float jit, int sty, int met) {
    int xi = 0;
    int yi = 0;
    int zi = 0;
    vec3 localpos = vec3(shadewire_floorfrac(p.x, xi), shadewire_floorfrac(p.y, yi), shadewire_floorfrac(p.z, zi));
    vec2 sqdist = vec2(1000000.0, 1000000.0);
    vec3 minpos = vec3(0.0, 0.0, 0.0);
    for (int x = -1; x <= 1; x++) {
        for (int y = -1; y <= 1; y++) {
            for (int z = -1; z <= 1; z++) {
                float dist = shadewire_worley_distance_3d(localpos, x, y, z, xi, yi, zi, jit, met);
                vec3 cellpos = shadewire_worley_cell_position_3d(x, y, z, xi, yi, zi, jit) - localpos;
                if (dist < sqdist.x) {
                    sqdist.y = sqdist.x;
                    sqdist.x = dist;
                    minpos = cellpos;
                } else if (dist < sqdist.y) {
                    sqdist.y = dist;
                }
            }
        }
    }
    if (sty == 1) {
        vec3 tmp = shadewire_cell_noise_vec3_3d(minpos + p);
        return vec2(tmp.x, tmp.y);
    }
    if (met == 0) {
        sqdist = sqrt(sqdist);
    }
    return sqdist;
}

// mx_worley_noise_vec3(vec3, float, int, int) (Rust worley_noise_vec3_3d)
vec3 shadewire_worley_noise_vec3_3d(vec3 p, float jit, int sty, int met) {
    int xi = 0;
    int yi = 0;
    int zi = 0;
    vec3 localpos = vec3(shadewire_floorfrac(p.x, xi), shadewire_floorfrac(p.y, yi), shadewire_floorfrac(p.z, zi));
    vec3 sqdist = vec3(1000000.0, 1000000.0, 1000000.0);
    vec3 minpos = vec3(0.0, 0.0, 0.0);
    for (int x = -1; x <= 1; x++) {
        for (int y = -1; y <= 1; y++) {
            for (int z = -1; z <= 1; z++) {
                float dist = shadewire_worley_distance_3d(localpos, x, y, z, xi, yi, zi, jit, met);
                vec3 cellpos = shadewire_worley_cell_position_3d(x, y, z, xi, yi, zi, jit) - localpos;
                if (dist < sqdist.x) {
                    sqdist.z = sqdist.y;
                    sqdist.y = sqdist.x;
                    sqdist.x = dist;
                    minpos = cellpos;
                } else if (dist < sqdist.y) {
                    sqdist.z = sqdist.y;
                    sqdist.y = dist;
                } else if (dist < sqdist.z) {
                    sqdist.z = dist;
                }
            }
        }
    }
    if (sty == 1) {
        return shadewire_cell_noise_vec3_3d(minpos + p);
    }
    if (met == 0) {
        sqdist = sqrt(sqdist);
    }
    return sqdist;
}
