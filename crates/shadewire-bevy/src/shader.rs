//! The runtime WESL module of one material: Bevy's PBR imports, the host functions, the generated WGSL
//! and a `fragment` entry in the shape of Bevy's own forward path (bevy_pbr 0.20.0 src/render/pbr.wesl).
//!
//! Imports, confirmed in bevy_pbr 0.20.0: `pbr_fragment::pbr_input_from_standard_material(in, is_front)`
//! (src/render/pbr_fragment.wesl:71), `pbr_functions::{alpha_discard, apply_pbr_lighting,
//! main_pass_post_lighting_processing, winding_corrected_front_facing}` (src/render/pbr_functions.wesl:98,
//! 326, 978, 139), `pbr_types::{STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT, STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
//! STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS, STANDARD_MATERIAL_FLAGS_ALPHA_MODE_BLEND,
//! STANDARD_MATERIAL_FLAGS_ALPHA_MODE_PREMULTIPLIED}` (src/render/pbr_types.wesl:39, 40, 56, 59, 60), `mesh_functions::{get_world_from_local, get_local_from_world}`
//! (src/render/mesh_functions.wesl:15, 25), `mesh_view_bindings::globals` (src/render/mesh_view_bindings.wesl:39),
//! `forward_io::{VertexOutput, FragmentOutput}` (src/render/forward_io.wesl:98, 118); under
//! `VISIBILITY_RANGE_DITHER`, `pbr_functions::visibility_range_dither` (pbr_functions.wesl:79), called first as `pbr.wesl:52-53` does.
//!
//! Forward only: `ShadewireExtension::specialize` swaps in this shader for the main pass and leaves the
//! prepass, shadow and deferred pipelines on Bevy's standard shaders, so there is no `PREPASS_PIPELINE`
//! branch. Not applied: clustered decals (`apply_decals`), forward decals and order-independent
//! transparency (`MATERIAL_OIT_ENABLED`), which Bevy's fragment handles after lighting.
//! The host reads `in.instance_index`, so the pipeline needs `VERTEX_OUTPUT_INSTANCE_INDEX`, which
//! `ShadewireExtension::specialize` adds. `tests/wesl_compiles.rs` compiles the module through wesl 0.6
//! against Bevy's own WESL sources and validates the result with naga.

use bevy::pbr::MATERIAL_BIND_GROUP_INDEX;
use shadewire::shader::names::ident;
use shadewire::{Reflection, model_struct_name};
use shadewire_wgsl::{CompileOptions, Compiled};

use crate::error::ShadewireError;
use crate::extension::FIRST_BINDING;
use crate::host::host;
use crate::lobes;
use crate::models::mapping;

/// Bevy's material bind group.
pub const MATERIAL_GROUP: u32 = MATERIAL_BIND_GROUP_INDEX as u32;

/// Compile options whose bindings match the extension layout: group 3, the uniform block at 100 (kept
/// even without uniforms), texture k at `101 + 2k`, its sampler at `102 + 2k`.
pub fn compile_options() -> CompileOptions {
    CompileOptions {
        group: MATERIAL_GROUP,
        first_binding: FIRST_BINDING,
        reserve_uniform_binding: true,
        ..CompileOptions::default()
    }
}

const IMPORTS: &str = "import bevy_pbr::render::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing, winding_corrected_front_facing},
    pbr_types::{
        STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT, STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
        STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS, STANDARD_MATERIAL_FLAGS_ALPHA_MODE_BLEND,
        STANDARD_MATERIAL_FLAGS_ALPHA_MODE_PREMULTIPLIED,
    },
    mesh_functions::{get_world_from_local, get_local_from_world},
    mesh_view_bindings::globals,
};

@if(VISIBILITY_RANGE_DITHER)
import bevy_pbr::render::pbr_functions::visibility_range_dither;
";

const ENTRY_HEAD: &str = "@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
@if(VISIBILITY_RANGE_DITHER)
    visibility_range_dither(in.position, in.visibility_range_dither);
    var pbr_input = pbr_input_from_standard_material(in, is_front);
";

const ENTRY_TAIL: &str = "    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color,
    );
    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
";

/// The bindings must be the extension's; anything else would read the wrong resources.
fn check_bindings(r: &Reflection) -> Result<(), ShadewireError> {
    if r.group != MATERIAL_GROUP {
        return Err(ShadewireError::Bindings(format!(
            "bind group {} is not the material group {MATERIAL_GROUP}",
            r.group
        )));
    }
    if let Some(b) = r.uniform_binding
        && b != FIRST_BINDING
    {
        return Err(ShadewireError::Bindings(format!("uniform block at binding {b}, not {FIRST_BINDING}")));
    }
    for (k, t) in r.textures.iter().enumerate() {
        let binding = FIRST_BINDING + 1 + 2 * k as u32;
        if t.binding != binding || t.sampler_binding != binding + 1 {
            return Err(ShadewireError::Bindings(format!(
                "texture {k} at bindings {}/{}, not {binding}/{}",
                t.binding,
                t.sampler_binding,
                binding + 1
            )));
        }
    }
    Ok(())
}

/// The WESL source of `compiled` (compiled with [`compile_options`]) as Bevy's main-pass fragment shader. A
/// named model is written onto Bevy's `PbrInput` and lit by Bevy's BRDF; a closure tree is lit by its own lobes
/// (`crate::lobes`).
pub fn module(compiled: &Compiled) -> Result<String, ShadewireError> {
    let reflection = &compiled.reflection;
    check_bindings(reflection)?;
    let closure = reflection.closure.as_ref();
    let map = if closure.is_some() { "" } else { mapping(&reflection.model)? };
    let host = host(reflection)?;
    let model = model_struct_name(&reflection.model);
    let mut out = String::new();
    out.push_str(IMPORTS);
    if closure.is_some() {
        out.push_str(lobes::IMPORTS);
    }
    out.push('\n');
    out.push_str(&host.declarations);
    out.push('\n');
    out.push_str(&compiled.code);
    out.push('\n');
    if closure.is_some() {
        out.push_str(&lobes::functions(&model));
        out.push('\n');
    }
    out.push_str(ENTRY_HEAD);
    out.push_str(&host.statements);
    out.push_str(&format!("    let s: {model} = shadewire_eval_surface();\n"));
    match closure {
        Some(graph) => out.push_str(&lobes::tail(graph.opacity.as_deref().map(ident).as_deref())),
        None => {
            out.push_str(map);
            out.push_str(ENTRY_TAIL);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shadewire_mtlx::{Resolver, to_ir};
    use shadewire_nodes::IrOptions;
    use shadewire_wgsl::compile;

    const UNLIT: &str = r#"<materialx version="1.39">
  <surface_unlit name="SU" type="surfaceshader">
    <input name="emission_color" type="color3" value="1.0, 0.5, 0.25" />
  </surface_unlit>
  <surfacematerial name="M" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="SU" />
  </surfacematerial>
</materialx>"#;

    fn compiled(text: &str, options: &CompileOptions) -> Compiled {
        let ir = to_ir(text, None, None, &IrOptions::default(), &Resolver::default()).unwrap();
        compile(&ir.dag, &ir.source, options).unwrap()
    }

    #[test]
    fn options_match_the_extension_layout() {
        let o = compile_options();
        assert_eq!((o.group, o.first_binding, o.reserve_uniform_binding, o.flip_v), (3, 100, true, true));
    }

    #[test]
    fn module_has_imports_generated_code_and_entry() {
        let c = compiled(UNLIT, &compile_options());
        let m = module(&c).unwrap();
        assert!(m.starts_with("import bevy_pbr::render::{"), "{m}");
        assert!(m.contains("fn shadewire_eval_surface() -> SurfaceUnlit {"), "{m}");
        assert!(m.contains("var pbr_input = pbr_input_from_standard_material(in, is_front);"), "{m}");
        assert!(m.contains("    let s: SurfaceUnlit = shadewire_eval_surface();\n"), "{m}");
        assert!(m.contains("pbr_input.material.flags |= STANDARD_MATERIAL_FLAGS_UNLIT_BIT;"), "{m}");
        assert!(m.contains("@group(3) @binding(100) var<uniform> shadewire_u"), "{m}");
    }

    #[test]
    fn forward_only_with_visibility_range_dither() {
        let m = module(&compiled(UNLIT, &compile_options())).unwrap();
        assert!(!m.contains("PREPASS_PIPELINE"), "{m}");
        assert!(m.contains(
            "@if(VISIBILITY_RANGE_DITHER)\nimport bevy_pbr::render::pbr_functions::visibility_range_dither;"
        ));
        assert!(m.contains(
            "@if(VISIBILITY_RANGE_DITHER)\n    visibility_range_dither(in.position, in.visibility_range_dither);"
        ));
        assert!(m.contains("winding_corrected_front_facing"), "{m}");
    }

    #[test]
    fn other_bindings_are_refused() {
        let c = compiled(UNLIT, &CompileOptions::default());
        let err = module(&c).unwrap_err();
        assert_eq!(
            err.to_string(),
            "bind group 2 is not the material group 3; compile with shadewire_bevy::compile_options()"
        );
    }
}
