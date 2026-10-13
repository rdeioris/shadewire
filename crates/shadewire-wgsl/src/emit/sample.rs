use shadewire::reflection::TextureReflection;
use shadewire::{AddressMode, Type};

use super::EmitOptions;

/// `textureSample` with MaterialX conventions: V flipped for top-row-first uploads,
/// `constant` address mode as an explicit bounds test returning `default`.
/// A `select` evaluates both sides, so the sample stays in uniform control flow.
/// `grad` (MaterialX uv space, like `uv`) makes it `textureSampleGrad`; the flip negates its v.
/// Without implicit derivatives a plain sample reads level 0 (`textureSampleLevel`).
pub fn sample_expr(
    t: &TextureReflection,
    uv: &str,
    ty: Type,
    default: &str,
    grad: Option<(&str, &str)>,
    options: &EmitOptions,
) -> String {
    let flip_v = options.flip_v;
    let v = if flip_v { format!("1.0 - {uv}.y") } else { format!("{uv}.y") };
    let st = format!("vec2<f32>({uv}.x, {v})");
    let flip = |d: &str| {
        if flip_v { format!("vec2<f32>({d}.x, -{d}.y)") } else { d.to_string() }
    };
    let swizzle = match ty.component_count() {
        1 => ".r",
        2 => ".rg",
        3 => ".rgb",
        _ => "",
    };
    let fetch = match grad {
        None if options.implicit_derivatives => {
            format!("textureSample(shadewire_t{i}, shadewire_s{i}, {st}){swizzle}", i = t.index)
        }
        None => format!("textureSampleLevel(shadewire_t{i}, shadewire_s{i}, {st}, 0.0){swizzle}", i = t.index),
        Some((dx, dy)) => {
            format!(
                "textureSampleGrad(shadewire_t{i}, shadewire_s{i}, {st}, {}, {}){swizzle}",
                flip(dx),
                flip(dy),
                i = t.index
            )
        }
    };
    let mut tests = Vec::new();
    if t.address[0] == AddressMode::Constant {
        tests.push(format!("{uv}.x < 0.0 || {uv}.x > 1.0"));
    }
    if t.address[1] == AddressMode::Constant {
        tests.push(format!("{uv}.y < 0.0 || {uv}.y > 1.0"));
    }
    if tests.is_empty() {
        return fetch;
    }
    format!("select({fetch}, {default}, {})", tests.join(" || "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shadewire::{AddressMode, FilterMode};

    fn texture(index: usize, u: AddressMode, v: AddressMode) -> TextureReflection {
        TextureReflection {
            index,
            binding: 0,
            sampler_binding: 1,
            file: "a.png".into(),
            colorspace: None,
            address: [u, v],
            filter: FilterMode::Linear,
        }
    }

    fn flat() -> EmitOptions {
        EmitOptions { flip_v: false, implicit_derivatives: true }
    }

    fn flipped() -> EmitOptions {
        EmitOptions { flip_v: true, implicit_derivatives: true }
    }

    #[test]
    fn without_implicit_derivatives_a_plain_sample_reads_level_zero() {
        let t = texture(0, P, P);
        let options = EmitOptions { flip_v: false, implicit_derivatives: false };
        assert_eq!(
            sample_expr(&t, "n3", Type::Float, "x", None, &options),
            "textureSampleLevel(shadewire_t0, shadewire_s0, vec2<f32>(n3.x, n3.y), 0.0).r"
        );
    }

    const P: AddressMode = AddressMode::Periodic;
    const C: AddressMode = AddressMode::Constant;

    #[test]
    fn plain_fetches_swizzle_to_the_node_width() {
        let t = texture(0, P, P);
        assert_eq!(
            sample_expr(&t, "n3", Type::Color3, "x", None, &flat()),
            "textureSample(shadewire_t0, shadewire_s0, vec2<f32>(n3.x, n3.y)).rgb"
        );
        assert_eq!(
            sample_expr(&t, "n3", Type::Float, "x", None, &flat()),
            "textureSample(shadewire_t0, shadewire_s0, vec2<f32>(n3.x, n3.y)).r"
        );
        assert_eq!(
            sample_expr(&t, "n3", Type::Vector2, "x", None, &flat()),
            "textureSample(shadewire_t0, shadewire_s0, vec2<f32>(n3.x, n3.y)).rg"
        );
        assert_eq!(
            sample_expr(&t, "n3", Type::Color4, "x", None, &flat()),
            "textureSample(shadewire_t0, shadewire_s0, vec2<f32>(n3.x, n3.y))"
        );
    }

    #[test]
    fn flip_v() {
        let t = texture(2, P, AddressMode::Clamp);
        assert_eq!(
            sample_expr(&t, "n3", Type::Color3, "x", None, &flipped()),
            "textureSample(shadewire_t2, shadewire_s2, vec2<f32>(n3.x, 1.0 - n3.y)).rgb"
        );
    }

    #[test]
    fn explicit_gradients_flip_with_v() {
        let t = texture(0, P, P);
        assert_eq!(
            sample_expr(&t, "n3", Type::Color3, "x", Some(("n4", "n5")), &flipped()),
            "textureSampleGrad(shadewire_t0, shadewire_s0, vec2<f32>(n3.x, 1.0 - n3.y), vec2<f32>(n4.x, -n4.y), vec2<f32>(n5.x, -n5.y)).rgb"
        );
        assert_eq!(
            sample_expr(&t, "n3", Type::Color4, "x", Some(("n4", "n5")), &flat()),
            "textureSampleGrad(shadewire_t0, shadewire_s0, vec2<f32>(n3.x, n3.y), n4, n5)"
        );
    }

    #[test]
    fn constant_address_is_a_bounds_test() {
        let t = texture(1, C, P);
        assert_eq!(
            sample_expr(&t, "n3", Type::Float, "0.5", None, &flat()),
            "select(textureSample(shadewire_t1, shadewire_s1, vec2<f32>(n3.x, n3.y)).r, 0.5, n3.x < 0.0 || n3.x > 1.0)"
        );
        let t = texture(1, C, C);
        assert_eq!(
            sample_expr(&t, "n3", Type::Float, "0.5", None, &flipped()),
            "select(textureSample(shadewire_t1, shadewire_s1, vec2<f32>(n3.x, 1.0 - n3.y)).r, 0.5, n3.x < 0.0 || n3.x > 1.0 || n3.y < 0.0 || n3.y > 1.0)"
        );
    }
}
