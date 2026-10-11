use shadewire_ir::reflection::TextureReflection;
use shadewire_ir::{AddressMode, Type};

/// `textureSample` with MaterialX conventions: V flipped for top-row-first uploads,
/// `constant` address mode as an explicit bounds test returning `default`.
/// A `select` evaluates both sides, so the sample stays in uniform control flow.
pub fn sample_expr(t: &TextureReflection, uv: &str, ty: Type, default: &str, flip_v: bool) -> String {
    let st = if flip_v { format!("vec2<f32>({uv}.x, 1.0 - {uv}.y)") } else { uv.to_string() };
    let swizzle = match ty.component_count() {
        1 => ".r",
        2 => ".rg",
        3 => ".rgb",
        _ => "",
    };
    let fetch = format!("textureSample(sw_t{i}, sw_s{i}, {st}){swizzle}", i = t.index);
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
    use shadewire_ir::{AddressMode, FilterMode};

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

    const P: AddressMode = AddressMode::Periodic;
    const C: AddressMode = AddressMode::Constant;

    #[test]
    fn plain_fetches_swizzle_to_the_node_width() {
        let t = texture(0, P, P);
        assert_eq!(sample_expr(&t, "n3", Type::Color3, "x", false), "textureSample(sw_t0, sw_s0, n3).rgb");
        assert_eq!(sample_expr(&t, "n3", Type::Float, "x", false), "textureSample(sw_t0, sw_s0, n3).r");
        assert_eq!(sample_expr(&t, "n3", Type::Vector2, "x", false), "textureSample(sw_t0, sw_s0, n3).rg");
        assert_eq!(sample_expr(&t, "n3", Type::Color4, "x", false), "textureSample(sw_t0, sw_s0, n3)");
    }

    #[test]
    fn flip_v() {
        let t = texture(2, P, AddressMode::Clamp);
        assert_eq!(
            sample_expr(&t, "n3", Type::Color3, "x", true),
            "textureSample(sw_t2, sw_s2, vec2<f32>(n3.x, 1.0 - n3.y)).rgb"
        );
    }

    #[test]
    fn constant_address_is_a_bounds_test() {
        let t = texture(1, C, P);
        assert_eq!(
            sample_expr(&t, "n3", Type::Float, "0.5", false),
            "select(textureSample(sw_t1, sw_s1, n3).r, 0.5, n3.x < 0.0 || n3.x > 1.0)"
        );
        let t = texture(1, C, C);
        assert_eq!(
            sample_expr(&t, "n3", Type::Float, "0.5", true),
            "select(textureSample(sw_t1, sw_s1, vec2<f32>(n3.x, 1.0 - n3.y)).r, 0.5, n3.x < 0.0 || n3.x > 1.0 || n3.y < 0.0 || n3.y > 1.0)"
        );
    }
}
