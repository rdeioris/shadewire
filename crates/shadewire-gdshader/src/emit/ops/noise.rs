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
                    "vec4(shadewire_perlin_noise_vec3_2d({p}), shadewire_perlin_noise_float_2d({p} + vec2(19.0, 73.0)))"
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
                    "vec4(shadewire_perlin_noise_vec3_3d({p}), shadewire_perlin_noise_float_3d({p} + vec3(19.0, 73.0, 29.0)))"
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

/// `value * amplitude + pivot`, amplitude broadcast to the result type. The fractal has no pivot.
fn scale(value: &str, amplitude: &Arg, pivot: Option<&str>, ty: Type) -> String {
    let work = work_type(ty);
    let amp = cast(&amplitude.0, amplitude.1, work);
    let expr = match pivot {
        Some(p) => format!("({value} * {amp} + {})", cast(p, Type::Float, work)),
        None => format!("({value} * {amp})"),
    };
    cast(&expr, work, ty)
}

/// Integer and enum inputs (octaves, style): an int as is, anything else as `int(first component)`,
/// like the interpreter's `as i32` (both round toward zero).
fn int(arg: &Arg) -> String {
    match arg.1 {
        Type::Integer => arg.0.clone(),
        _ => format!("int({})", first_f32(arg)),
    }
}
