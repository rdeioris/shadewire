//! A closure tree (the `surface` model) lit by MaterialX's reference lobes in Bevy's forward pass: the fragment
//! entry calls `shadewire_light` instead of `apply_pbr_lighting`, which walks Bevy's lights as
//! `apply_pbr_lighting` does (bevy_pbr 0.20.0 src/render/pbr_functions.wesl:326) and adds
//! `shadewire_eval_bsdf`'s response for each, then `shadewire_eval_emission`.
//!
//! Lights, all from bevy_pbr 0.20.0:
//!
//! | light | direction and radiance |
//! |---|---|
//! | point | the clustered lights from `first_point_light_index_offset` to `first_spot_light_index_offset` (pbr_functions.wesl:461); `color_inverse_square_range.rgb × getDistanceAttenuation(d², color_inverse_square_range.w)` toward `position_radius.xyz` (render/pbr_lighting.wesl:645-649, 791-792) |
//! | spot | the clustered lights up to `first_rect_light_index_offset` (pbr_functions.wesl:507): the point radiance times the cone attenuation of `spot_light` (pbr_lighting.wesl:795-822) |
//! | directional | `lights.directional_lights[i]` (pbr_functions.wesl:563): `color.rgb` from `direction_to_light` (pbr_lighting.wesl:855, 921) |
//!
//! Bevy premultiplies a point light's color by its intensity over 4π on the CPU and scales its BRDFs by `NdotL`
//! with Lambert's 1/π inside (`Fd_Burley`, pbr_lighting.wesl:510-524); a MaterialX response carries the cosine and
//! the 1/π, so it multiplies the radiance as is and a white Lambert lobe lights as a white rough
//! `StandardMaterial` does. Shadows and contact shadows are fetched as `apply_pbr_lighting` fetches them
//! (pbr_functions.wesl:479-489, 524-542, 579-591). Exposure scales direct light; emission is scaled by the alpha
//! and by `mix(1, exposure, emissive.a)` of the base material, as Bevy scales `emissive`
//! (pbr_functions.wesl:822-833).
//!
//! Not lit: rect lights (their LTC integral needs Bevy's BRDF), light textures, the atmosphere's transmittance of
//! directional lights, and indirect light. Point and spot lights are points: their radius does not widen the
//! lobes as Bevy's `compute_specular_layer_values_for_point_light` widens its own.

/// The imports `shadewire_light` adds to the module's.
pub const IMPORTS: &str = "import bevy_pbr::render::{
    pbr_types::PbrInput,
    mesh_view_bindings as view_bindings,
    mesh_view_types,
    pbr_lighting as lighting,
    clustered_forward as clustering,
    shadows,
    mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT,
};

@if(CONTACT_SHADOWS && DEPTH_PREPASS)
import bevy_pbr::render::pbr_functions::calculate_contact_shadow;
";

/// What a closure tree in Bevy is not lit by; the reflection's `approximations` gets it.
pub const APPROXIMATIONS: &str = "bevy: a closure tree is lit by point, spot and directional lights only (as points, \
     without rect lights, light textures, atmosphere or indirect light)";

/// The contact shadow of a light toward `l` when the light casts them (`flags & bit`), else 1.
const CONTACT: &str = "@if(CONTACT_SHADOWS && DEPTH_PREPASS)
fn shadewire_contact(in: PbrInput, shadow: f32, flags: u32, bit: u32, l: vec3<f32>) -> f32 {
    let steps = view_bindings::contact_shadows_settings.linear_steps;
    if steps > 0u && (in.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u && shadow > 0.0 && (flags & bit) != 0u {
        return calculate_contact_shadow(in.world_position.xyz, in.frag_coord.xy, l, steps);
    }
    return 1.0;
}
";

/// `fn shadewire_light(in: PbrInput, s: {MODEL}, n: vec3<f32>) -> vec4<f32>`: the lit color, its alpha
/// `in.material.base_color.a`. `n` is the shading normal the lobes fall back to.
const LIGHT: &str = "fn shadewire_light(in: PbrInput, s: {MODEL}, n: vec3<f32>) -> vec4<f32> {
    let v = in.V;
    let p = in.world_position.xyz;
    let receiver = (in.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u;
    let view_z = dot(vec4<f32>(
        view_bindings::view.view_from_world[0].z,
        view_bindings::view.view_from_world[1].z,
        view_bindings::view.view_from_world[2].z,
        view_bindings::view.view_from_world[3].z
    ), in.world_position);
    let cluster_index = clustering::view_fragment_cluster_index(in.frag_coord.xy, view_z, in.is_orthographic);
    let ranges = clustering::unpack_clusterable_object_index_ranges(cluster_index);
    var direct = vec3<f32>(0.0);

    // Point and spot lights: the spot lights follow the point lights in the cluster's list.
    for (var i: u32 = ranges.first_point_light_index_offset; i < ranges.first_rect_light_index_offset; i = i + 1u) {
        let id = clustering::get_clusterable_object_id(i);
        let light = &view_bindings::clustered_lights.data[id];
        let to_light = (*light).position_radius.xyz - p;
        let d2 = dot(to_light, to_light);
        let l = to_light * inverseSqrt(d2);
        var radiance = (*light).color_inverse_square_range.rgb
            * lighting::getDistanceAttenuation(d2, (*light).color_inverse_square_range.w);
        let spot = i >= ranges.first_spot_light_index_offset;
        if spot {
            var spot_dir = vec3<f32>((*light).light_custom_data.x, 0.0, (*light).light_custom_data.y);
            spot_dir.y = sqrt(max(0.0, 1.0 - spot_dir.x * spot_dir.x - spot_dir.z * spot_dir.z));
            if ((*light).flags & mesh_view_types::POINT_LIGHT_FLAGS_SPOT_LIGHT_Y_NEGATIVE) != 0u {
                spot_dir.y = -spot_dir.y;
            }
            let cone = saturate(dot(-spot_dir, l) * (*light).light_custom_data.z + (*light).light_custom_data.w);
            radiance *= cone * cone;
        }
        var shadow = 1.0;
        if receiver && ((*light).flags & mesh_view_types::POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u {
            if spot {
                shadow = shadows::fetch_spot_shadow(id, in.world_position, in.world_normal, (*light).shadow_map_near_z, in.frag_coord.xy);
            } else {
                shadow = shadows::fetch_point_shadow(id, in.world_position, in.world_normal, in.frag_coord.xy);
            }
        }
        @if(CONTACT_SHADOWS && DEPTH_PREPASS)
        shadow *= shadewire_contact(in, shadow, (*light).flags, mesh_view_types::POINT_LIGHT_FLAGS_CONTACT_SHADOWS_ENABLED_BIT, l);
        direct += shadewire_eval_bsdf(s, l, v, n).response * radiance * shadow;
    }

    for (var i: u32 = 0u; i < view_bindings::lights.n_directional_lights; i = i + 1u) {
        let light = &view_bindings::lights.directional_lights[i];
        let l = (*light).direction_to_light.xyz;
        var shadow = 1.0;
        if receiver && ((*light).flags & mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u {
            shadow = shadows::fetch_directional_shadow(i, in.world_position, in.world_normal, view_z, in.frag_coord.xy);
            shadow *= in.directional_shadow_factor;
        }
        @if(CONTACT_SHADOWS && DEPTH_PREPASS)
        shadow *= shadewire_contact(in, shadow, (*light).flags, mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_CONTACT_SHADOWS_ENABLED_BIT, l);
        direct += shadewire_eval_bsdf(s, l, v, n).response * (*light).color.rgb * shadow;
    }

    let alpha = in.material.base_color.a;
    let exposure = view_bindings::view.exposure;
    let emission = shadewire_eval_emission(s, v, n) * alpha * mix(1.0, exposure, in.material.emissive.a);
    return vec4<f32>(exposure * direct + emission, alpha);
}
";

/// The lighting functions for a closure tree whose model struct is `model`.
pub fn functions(model: &str) -> String {
    format!("{CONTACT}\n{}", LIGHT.replace("{MODEL}", model))
}

/// The fragment entry's statements after `let s = shadewire_eval_surface();`: the opacity as the alpha (1 when the
/// tree has none), alpha discard, the lit color and Bevy's post-lighting processing.
pub fn tail(opacity: Option<&str>) -> String {
    let alpha = opacity.map_or("1.0".to_string(), |field| format!("s.{field}"));
    format!(
        "    pbr_input.material.base_color = vec4<f32>(pbr_input.material.base_color.rgb, {alpha});
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color,
    );
    var out: FragmentOutput;
    out.color = shadewire_light(pbr_input, s, shadewire_n);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}}
"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_model_struct_is_named() {
        let f = functions("Surface");
        assert!(f.contains("fn shadewire_light(in: PbrInput, s: Surface, n: vec3<f32>) -> vec4<f32> {"), "{f}");
        assert!(!f.contains("{MODEL}"));
    }

    #[test]
    fn the_opacity_field_is_the_alpha() {
        assert!(tail(Some("opacity")).contains("vec4<f32>(pbr_input.material.base_color.rgb, s.opacity);"));
        assert!(tail(None).contains("vec4<f32>(pbr_input.material.base_color.rgb, 1.0);"));
    }
}
