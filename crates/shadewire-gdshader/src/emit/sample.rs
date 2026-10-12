use shadewire::reflection::TextureReflection;
use shadewire::{AddressMode, FilterMode, Type};

/// Whether the sampler repeats: Godot sets one repeat flag per texture, so a texture repeats
/// when either axis is periodic, and the other axis is emulated in the shader.
pub fn repeats(t: &TextureReflection) -> bool {
    t.address.contains(&AddressMode::Periodic)
}

/// `uniform sampler2D shadewire_t<i> : <filter>, <repeat>;`. No `source_color`: the generated code
/// converts colorspaces itself, so Godot must not decode the texels.
pub fn texture_uniform(t: &TextureReflection) -> String {
    let filter = match t.filter {
        FilterMode::Closest => "filter_nearest_mipmap",
        FilterMode::Linear | FilterMode::Cubic => "filter_linear_mipmap_anisotropic",
    };
    let repeat = if repeats(t) { "repeat_enable" } else { "repeat_disable" };
    format!("uniform sampler2D shadewire_t{} : {filter}, {repeat};\n", t.index)
}

/// One axis of the lookup coordinate in MaterialX uv space. Mirror is emulated (Godot has no
/// mirrored repeat); clamp and constant on a repeating sampler clamp in the shader.
fn axis(t: &TextureReflection, coord: &str, mode: AddressMode) -> (String, bool) {
    match mode {
        AddressMode::Periodic => (coord.to_string(), false),
        AddressMode::Mirror => (format!("(1.0 - abs(mod({coord}, 2.0) - 1.0))"), true),
        AddressMode::Clamp | AddressMode::Constant if repeats(t) => (format!("clamp({coord}, 0.0, 1.0)"), true),
        AddressMode::Clamp | AddressMode::Constant => (coord.to_string(), false),
    }
}

/// `texture` with MaterialX conventions: V flipped (Godot images are top row first, MaterialX
/// texcoords have V up), `constant` address mode as an explicit bounds test returning `default`.
/// The test is a `mix` by 0 or 1, which evaluates both sides (a ternary would not), so the lookup
/// stays in uniform control flow; `mix` by exactly 1 returns `default` unchanged. `grad` (MaterialX uv space, like `uv`) makes it `textureGrad`; the flip negates
/// its v. An emulated axis also samples with `textureGrad`, on the derivatives of the coordinate
/// before emulation, so the mip level does not jump at the seams.
pub fn sample_expr(t: &TextureReflection, uv: &str, ty: Type, default: &str, grad: Option<(&str, &str)>) -> String {
    let (u, emulated_u) = axis(t, &format!("{uv}.x"), t.address[0]);
    let (v, emulated_v) = axis(t, &format!("{uv}.y"), t.address[1]);
    let st = format!("vec2({u}, 1.0 - {v})");
    let swizzle = match ty.component_count() {
        1 => ".r",
        2 => ".rg",
        3 => ".rgb",
        _ => "",
    };
    let flip = |d: &str| format!("vec2({d}.x, -{d}.y)");
    let i = t.index;
    let fetch = match grad {
        Some((dx, dy)) => format!("textureGrad(shadewire_t{i}, {st}, {}, {}){swizzle}", flip(dx), flip(dy)),
        None if emulated_u || emulated_v => {
            let raw = format!("vec2({uv}.x, 1.0 - {uv}.y)");
            format!("textureGrad(shadewire_t{i}, {st}, dFdx({raw}), dFdy({raw})){swizzle}")
        }
        None => format!("texture(shadewire_t{i}, {st}){swizzle}"),
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
    format!("mix({fetch}, {default}, (({}) ? 1.0 : 0.0))", tests.join(" || "))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    const M: AddressMode = AddressMode::Mirror;
    const L: AddressMode = AddressMode::Clamp;

    #[test]
    fn plain_fetches_flip_v_and_swizzle() {
        let t = texture(0, P, P);
        assert_eq!(sample_expr(&t, "n3", Type::Color3, "x", None), "texture(shadewire_t0, vec2(n3.x, 1.0 - n3.y)).rgb");
        assert_eq!(sample_expr(&t, "n3", Type::Float, "x", None), "texture(shadewire_t0, vec2(n3.x, 1.0 - n3.y)).r");
        assert_eq!(sample_expr(&t, "n3", Type::Color4, "x", None), "texture(shadewire_t0, vec2(n3.x, 1.0 - n3.y))");
    }

    #[test]
    fn explicit_gradients_flip_with_v() {
        let t = texture(0, P, P);
        assert_eq!(
            sample_expr(&t, "n3", Type::Color3, "x", Some(("n4", "n5"))),
            "textureGrad(shadewire_t0, vec2(n3.x, 1.0 - n3.y), vec2(n4.x, -n4.y), vec2(n5.x, -n5.y)).rgb"
        );
    }

    #[test]
    fn constant_address_is_a_bounds_test() {
        let t = texture(1, C, L);
        assert_eq!(
            sample_expr(&t, "n3", Type::Float, "0.5", None),
            "mix(texture(shadewire_t1, vec2(n3.x, 1.0 - n3.y)).r, 0.5, ((n3.x < 0.0 || n3.x > 1.0) ? 1.0 : 0.0))"
        );
        assert_eq!(
            sample_expr(&texture(1, C, C), "n3", Type::Color3, "vec3(0.5)", None),
            "mix(texture(shadewire_t1, vec2(n3.x, 1.0 - n3.y)).rgb, vec3(0.5), ((n3.x < 0.0 || n3.x > 1.0 || n3.y < 0.0 || n3.y > 1.0) ? 1.0 : 0.0))"
        );
    }

    #[test]
    fn mirror_and_mixed_axes_are_emulated() {
        let t = texture(2, M, P);
        assert_eq!(
            sample_expr(&t, "n3", Type::Float, "0.0", None),
            "textureGrad(shadewire_t2, vec2((1.0 - abs(mod(n3.x, 2.0) - 1.0)), 1.0 - n3.y), dFdx(vec2(n3.x, 1.0 - n3.y)), dFdy(vec2(n3.x, 1.0 - n3.y))).r"
        );
        let t = texture(0, P, L);
        assert_eq!(
            sample_expr(&t, "n3", Type::Float, "0.0", None),
            "textureGrad(shadewire_t0, vec2(n3.x, 1.0 - clamp(n3.y, 0.0, 1.0)), dFdx(vec2(n3.x, 1.0 - n3.y)), dFdy(vec2(n3.x, 1.0 - n3.y))).r"
        );
    }

    #[test]
    fn uniform_hints() {
        assert_eq!(
            texture_uniform(&texture(3, P, L)),
            "uniform sampler2D shadewire_t3 : filter_linear_mipmap_anisotropic, repeat_enable;\n"
        );
        let mut t = texture(0, L, M);
        t.filter = FilterMode::Closest;
        assert_eq!(texture_uniform(&t), "uniform sampler2D shadewire_t0 : filter_nearest_mipmap, repeat_disable;\n");
    }
}
