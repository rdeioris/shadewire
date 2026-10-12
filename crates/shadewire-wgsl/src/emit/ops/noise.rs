use shadewire::{Op, Type};

use super::{Arg, cast, first_f32, leading, work_type};

/// Mirrors `shadewire::ops::noise::eval`: the genglsl node formulas over the leaf library.
/// Arguments arrive in `catalog::input_names` order.
pub fn emit(op: &Op, args: &[Arg], ty: Type) -> String {
    match op {
        Op::Noise2d => {
            let p = leading(&args[2], 2);
            let value = match ty.component_count() {
                1 => format!("shadewire_perlin_noise_float_2d({p})"),
                2 => format!("shadewire_perlin_noise_vec3_2d({p}).xy"),
                3 => format!("shadewire_perlin_noise_vec3_2d({p})"),
                _ => format!(
                    "vec4<f32>(shadewire_perlin_noise_vec3_2d({p}), shadewire_perlin_noise_float_2d({p} + vec2<f32>(19.0, 73.0)))"
                ),
            };
            scale(&value, &args[0], Some(&first_f32(&args[1])), ty)
        }
        Op::Noise3d => {
            let p = leading(&args[2], 3);
            let value = match ty.component_count() {
                1 => format!("shadewire_perlin_noise_float_3d({p})"),
                2 => format!("shadewire_perlin_noise_vec3_3d({p}).xy"),
                3 => format!("shadewire_perlin_noise_vec3_3d({p})"),
                _ => format!(
                    "vec4<f32>(shadewire_perlin_noise_vec3_3d({p}), shadewire_perlin_noise_float_3d({p} + vec3<f32>(19.0, 73.0, 29.0)))"
                ),
            };
            scale(&value, &args[0], Some(&first_f32(&args[1])), ty)
        }
        Op::Fractal3d => {
            let p = leading(&args[4], 3);
            let rest = format!("{p}, {}, {}, {}", int(&args[1]), first_f32(&args[2]), first_f32(&args[3]));
            let suffix = match ty.component_count() {
                1 => "float",
                2 => "vec2",
                3 => "vec3",
                _ => "vec4",
            };
            scale(&format!("shadewire_fractal_noise_{suffix}_3d({rest})"), &args[0], None, ty)
        }
        Op::CellNoise2d => cast(&format!("shadewire_cell_noise_float_2d({})", leading(&args[0], 2)), Type::Float, ty),
        Op::CellNoise3d => cast(&format!("shadewire_cell_noise_float_3d({})", leading(&args[0], 3)), Type::Float, ty),
        Op::WorleyNoise2d => worley(&leading(&args[0], 2), "2d", args, ty),
        Op::WorleyNoise3d => worley(&leading(&args[0], 3), "3d", args, ty),
        other => panic!("{other:?} is not a noise op"),
    }
}

/// mx_worleynoise{2d,3d}_*: metric is always 0 (Euclidean). Any width above two reads
/// the vec3 variant, as the interpreter's `_` arm does.
fn worley(p: &str, dim: &str, args: &[Arg], ty: Type) -> String {
    let (suffix, result) = match ty.component_count() {
        1 => ("float", Type::Float),
        2 => ("vec2", Type::Vector2),
        _ => ("vec3", Type::Vector3),
    };
    let call = format!("shadewire_worley_noise_{suffix}_{dim}({p}, {}, {}, 0)", first_f32(&args[1]), int(&args[2]));
    cast(&call, result, ty)
}

/// `value * amplitude + pivot`, amplitude broadcast to the result type. The fractal has no
/// pivot: the interpreter's `+ 0.0` only turns -0.0 into +0.0, which compare equal.
fn scale(value: &str, amplitude: &Arg, pivot: Option<&str>, ty: Type) -> String {
    let work = work_type(ty);
    let amp = cast(&amplitude.0, amplitude.1, work);
    let expr = match pivot {
        Some(p) => format!("({value} * {amp} + {})", cast(p, Type::Float, work)),
        None => format!("({value} * {amp})"),
    };
    cast(&expr, work, ty)
}

/// Integer and enum inputs (octaves, style): an i32 as is, anything else as `i32(first component)`,
/// like the interpreter's `as i32` (both round toward zero).
fn int(arg: &Arg) -> String {
    match arg.1 {
        Type::Integer => arg.0.clone(),
        _ => format!("i32({})", first_f32(arg)),
    }
}

#[cfg(test)]
mod tests {
    use super::emit;
    use crate::emit::test_support::args;
    use shadewire::{Op, Type};

    const F: Type = Type::Float;
    const I: Type = Type::Integer;
    const V2: Type = Type::Vector2;
    const V3: Type = Type::Vector3;
    const V4: Type = Type::Vector4;

    #[test]
    fn noise2d_widths() {
        assert_eq!(emit(&Op::Noise2d, &args(&[F, F, V2]), F), "(shadewire_perlin_noise_float_2d(n2) * n0 + n1)");
        assert_eq!(
            emit(&Op::Noise2d, &args(&[F, F, V2]), V2),
            "(shadewire_perlin_noise_vec3_2d(n2).xy * vec2<f32>(n0) + vec2<f32>(n1))"
        );
        assert_eq!(
            emit(&Op::Noise2d, &args(&[V4, F, V2]), V4),
            "(vec4<f32>(shadewire_perlin_noise_vec3_2d(n2), shadewire_perlin_noise_float_2d(n2 + vec2<f32>(19.0, 73.0))) * n0 + vec4<f32>(n1))"
        );
    }

    #[test]
    fn noise3d_offsets_w_and_reads_the_pivot_first_component() {
        assert_eq!(
            emit(&Op::Noise3d, &args(&[F, V2, V3]), V4),
            "(vec4<f32>(shadewire_perlin_noise_vec3_3d(n2), shadewire_perlin_noise_float_3d(n2 + vec3<f32>(19.0, 73.0, 29.0))) * vec4<f32>(n0) + vec4<f32>(n1.x))"
        );
        assert_eq!(
            emit(&Op::Noise3d, &args(&[V3, F, V3]), V3),
            "(shadewire_perlin_noise_vec3_3d(n2) * n0 + vec3<f32>(n1))"
        );
    }

    #[test]
    fn fractal3d_has_no_pivot_and_integer_octaves() {
        assert_eq!(
            emit(&Op::Fractal3d, &args(&[F, I, F, F, V3]), F),
            "(shadewire_fractal_noise_float_3d(n4, n1, n2, n3) * n0)"
        );
        assert_eq!(
            emit(&Op::Fractal3d, &args(&[F, F, F, F, V3]), V2),
            "(shadewire_fractal_noise_vec2_3d(n4, i32(n1), n2, n3) * vec2<f32>(n0))"
        );
    }

    #[test]
    fn cell_noise_reads_the_leading_components() {
        assert_eq!(emit(&Op::CellNoise2d, &args(&[V2]), F), "shadewire_cell_noise_float_2d(n0)");
        assert_eq!(emit(&Op::CellNoise3d, &args(&[V4]), F), "shadewire_cell_noise_float_3d(n0.xyz)");
    }

    #[test]
    fn worley_uses_metric_zero() {
        assert_eq!(emit(&Op::WorleyNoise2d, &args(&[V2, F, I]), F), "shadewire_worley_noise_float_2d(n0, n1, n2, 0)");
        assert_eq!(emit(&Op::WorleyNoise3d, &args(&[V3, F, I]), V3), "shadewire_worley_noise_vec3_3d(n0, n1, n2, 0)");
        assert_eq!(
            emit(&Op::WorleyNoise3d, &args(&[V3, F, F]), V2),
            "shadewire_worley_noise_vec2_3d(n0, n1, i32(n2), 0)"
        );
    }
}
