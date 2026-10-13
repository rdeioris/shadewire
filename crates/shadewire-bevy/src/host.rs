//! The `shadewire_host_*` functions, defined from Bevy's forward `VertexOutput`.
//!
//! Every read is a `var<private>` the fragment entry sets before `shadewire_eval_surface()` runs, and
//! a function returning it (the reference host's shape). Sources, all from bevy_pbr 0.20.0:
//!
//! | read | source |
//! |---|---|
//! | `position_world` | `in.world_position.xyz` (`VertexOutput.world_position`, render/forward_io.wesl:102) |
//! | `position_object`, `position_model` | world position through `get_local_from_world(in.instance_index)` (render/mesh_functions.wesl:25; `instance_index` needs `VERTEX_OUTPUT_INSTANCE_INDEX`, forward_io.wesl:112-113) |
//! | `normal_world` | `normalize(in.world_normal)` (forward_io.wesl:103), negated on the back face of a double-sided material: back = `!winding_corrected_front_facing(pbr_input.flags, is_front)` (render/pbr_functions.wesl:139), double-sided = `STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT` (pbr_types.wesl:39) in `pbr_input.material.flags`. Bevy's own `prepare_world_normal` (pbr_functions.wesl:126-136) flips only without `VERTEX_TANGENTS` and `STANDARD_MATERIAL_NORMAL_MAP`, so the flip is done here, the same in every pipeline |
//! | `normal_object`, `normal_model` | the world normal through the inverse transpose of local-from-world, `n * mat3(world_from_local)` |
//! | `tangent_world`, `bitangent_world` | `in.world_tangent` (`VERTEX_TANGENTS`, forward_io.wesl:108-109), orthogonalised against N, bitangent `w * cross(N, T)` as in `calculate_tbn_mikktspace` (pbr_functions.wesl:148); without tangents a screen-space frame from position and MaterialX UV derivatives, `w` chosen so the bitangent runs along +V. T and B are negated with N on back faces |
//! | `texcoord_0`, `texcoord_1` | `vec2(uv.x, 1.0 - uv.y)` from `in.uv` / `in.uv_b` (`VERTEX_UVS_A` / `VERTEX_UVS_B`, forward_io.wesl:104-107), else zero: glTF V points down, MaterialX V up |
//! | `geomcolor_0` | `in.color` (`VERTEX_COLORS`, forward_io.wesl:110-111), else white |
//! | `view_direction_world` | `-pbr_input.V`: Bevy's `V` (pbr_types.wesl:111) points from the fragment to the camera, so `-V` runs from the camera to the surface, MaterialX's view direction |
//! | `time`, `frame` | `globals.time`, `f32(globals.frame_count)` (render/mesh_view_bindings.wesl:39, bevy_render globals.wesl) |
//! | `transform_*` | `get_world_from_local` / `get_local_from_world` (mesh_functions.wesl:15, 25); Bevy has no model space apart from object space |
//! | `geomprop_*` | the reflection's authored default, else zero (no mesh attribute plumbing) |
//!
//! Anything else is `ShadewireError::UnsupportedHost` naming it.

use shadewire::{HOST_PREFIX, Reflection, Type, Value};
use shadewire_wgsl::{literal, wgsl_type};

use crate::error::ShadewireError;

/// Declarations at module scope and the statements the fragment entry runs before evaluating the
/// surface. The statements read `in` (the `VertexOutput`) and `pbr_input`.
pub struct Host {
    pub declarations: String,
    pub statements: String,
}

/// Helpers and the frame every read is taken from, always present (WGSL drops what is unused).
const HELPERS: &str = "var<private> shadewire_world_from_local: mat4x4<f32>;
var<private> shadewire_local_from_world: mat4x4<f32>;

fn shadewire_mat3(m: mat4x4<f32>) -> mat3x3<f32> {
    return mat3x3<f32>(m[0].xyz, m[1].xyz, m[2].xyz);
}

fn shadewire_safe_normalize(v: vec3<f32>) -> vec3<f32> {
    let l = length(v);
    if l > 0.0 {
        return v / l;
    }
    return v;
}

/// Any unit vector perpendicular to `n`.
fn shadewire_perpendicular(n: vec3<f32>) -> vec3<f32> {
    let axis = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), abs(n.x) > 0.9);
    return normalize(cross(axis, n));
}

/// A tangent along +U from screen-space derivatives of position and (MaterialX, V-up) UV, for meshes
/// without tangents; `w` is the handedness that makes `w * cross(n, t)` run along +V.
fn shadewire_screen_tangent(n: vec3<f32>, p: vec3<f32>, uv: vec2<f32>) -> vec4<f32> {
    let dp1 = dpdx(p);
    let dp2 = dpdy(p);
    let duv1 = dpdx(uv);
    let duv2 = dpdy(uv);
    let a = cross(dp2, n);
    let c = cross(n, dp1);
    let t = a * duv1.x + c * duv2.x;
    let b = a * duv1.y + c * duv2.y;
    if dot(t, t) > 1e-20 {
        let w = select(1.0, -1.0, dot(cross(n, t), b) < 0.0);
        return vec4<f32>(normalize(t), w);
    }
    return vec4<f32>(shadewire_perpendicular(n), 1.0);
}
";

/// The frame statements: matrices, UVs, color, then N, T, B. The frame is built from the vertex normal
/// and negated as a whole on the back face of a double-sided material: Bevy's `prepare_world_normal`
/// flips `pbr_input.world_normal` only without tangents and normal maps, so it is not used here.
const FRAME: &str = "    shadewire_world_from_local = get_world_from_local(in.instance_index);
    shadewire_local_from_world = get_local_from_world(in.instance_index);
@if(VERTEX_UVS_A)
    let shadewire_uv0 = in.uv;
@else
    let shadewire_uv0 = vec2<f32>(0.0);
@if(VERTEX_UVS_B)
    let shadewire_uv1 = in.uv_b;
@else
    let shadewire_uv1 = vec2<f32>(0.0);
@if(VERTEX_COLORS)
    let shadewire_color0 = in.color;
@else
    let shadewire_color0 = vec4<f32>(1.0);
    let shadewire_front = winding_corrected_front_facing(pbr_input.flags, is_front);
    let shadewire_double_sided = (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT) != 0u;
    let shadewire_side = select(1.0, -1.0, shadewire_double_sided && !shadewire_front);
    let shadewire_n0 = normalize(in.world_normal);
@if(VERTEX_TANGENTS)
    let shadewire_t4 = in.world_tangent;
@else
    let shadewire_t4 = shadewire_screen_tangent(shadewire_n0, in.world_position.xyz, vec2<f32>(shadewire_uv0.x, 1.0 - shadewire_uv0.y));
    let shadewire_t0 = shadewire_safe_normalize(shadewire_t4.xyz - shadewire_n0 * dot(shadewire_n0, shadewire_t4.xyz));
    let shadewire_n = shadewire_side * shadewire_n0;
    let shadewire_t = shadewire_side * shadewire_t0;
    let shadewire_b = shadewire_side * select(1.0, -1.0, shadewire_t4.w < 0.0) * cross(shadewire_n0, shadewire_t0);
";

const TO_OBJECT: &str = "shadewire_mat3(shadewire_local_from_world)";

/// The WGSL expression for a varying, derived or global read, or `None` when Bevy has no source.
fn source(name: &str, ty: Type) -> Option<String> {
    let vec3 = ty == Type::Vector3;
    let expr = match name {
        "position_world" if vec3 => "in.world_position.xyz".to_string(),
        "position_object" | "position_model" if vec3 => {
            "(shadewire_local_from_world * vec4<f32>(in.world_position.xyz, 1.0)).xyz".to_string()
        }
        "normal_world" if vec3 => "shadewire_n".to_string(),
        "normal_object" | "normal_model" if vec3 => {
            "shadewire_safe_normalize(shadewire_n * shadewire_mat3(shadewire_world_from_local))".to_string()
        }
        "tangent_world" if vec3 => "shadewire_t".to_string(),
        "tangent_object" | "tangent_model" if vec3 => format!("shadewire_safe_normalize({TO_OBJECT} * shadewire_t)"),
        "bitangent_world" if vec3 => "shadewire_b".to_string(),
        "bitangent_object" | "bitangent_model" if vec3 => {
            format!("shadewire_safe_normalize({TO_OBJECT} * shadewire_b)")
        }
        "view_direction_world" if vec3 => "-pbr_input.V".to_string(),
        "view_direction_object" | "view_direction_model" if vec3 => {
            format!("shadewire_safe_normalize({TO_OBJECT} * -pbr_input.V)")
        }
        "texcoord_0" | "texcoord_1" => {
            let uv = if name == "texcoord_0" { "shadewire_uv0" } else { "shadewire_uv1" };
            match ty {
                Type::Vector2 => format!("vec2<f32>({uv}.x, 1.0 - {uv}.y)"),
                Type::Vector3 => format!("vec3<f32>({uv}.x, 1.0 - {uv}.y, 0.0)"),
                _ => return None,
            }
        }
        "geomcolor_0" => match ty {
            Type::Float => "shadewire_color0.r".to_string(),
            Type::Color3 => "shadewire_color0.rgb".to_string(),
            Type::Color4 => "shadewire_color0".to_string(),
            _ => return None,
        },
        "time" if ty == Type::Float => "globals.time".to_string(),
        "frame" if ty == Type::Float => "f32(globals.frame_count)".to_string(),
        _ => return None,
    };
    Some(expr)
}

/// `object` or `world`: Bevy's model space is its object space.
fn space(name: &str) -> Option<&'static str> {
    match name {
        "model" | "object" => Some("object"),
        "world" => Some("world"),
        _ => None,
    }
}

/// The body of `transform_<kind>_<from>_to_<to>(v)`, or `None` for a name it does not know.
/// Normals go through the inverse transpose; the generated code normalises them.
fn transform_body(name: &str) -> Option<String> {
    let rest = name.strip_prefix("transform_")?;
    let (kind, spaces) = rest.split_once('_')?;
    let (from, to) = spaces.split_once("_to_")?;
    let (from, to) = (space(from)?, space(to)?);
    if from == to {
        return matches!(kind, "point" | "vector" | "normal").then(|| "v".to_string());
    }
    let (forward, inverse) = if to == "world" {
        ("shadewire_world_from_local", "shadewire_local_from_world")
    } else {
        ("shadewire_local_from_world", "shadewire_world_from_local")
    };
    match kind {
        "point" => Some(format!("({forward} * vec4<f32>(v, 1.0)).xyz")),
        "vector" => Some(format!("shadewire_mat3({forward}) * v")),
        "normal" => Some(format!("v * shadewire_mat3({inverse})")),
        _ => None,
    }
}

fn read(out: &mut String, function: &str, ty: Type, init: Option<&Value>) {
    let var = format!("shadewire_in_{}", function.strip_prefix(HOST_PREFIX).unwrap_or(function));
    let ty = wgsl_type(ty);
    let start = init.map_or(String::new(), |v| format!(" = {}", literal(v)));
    out.push_str(&format!("var<private> {var}: {ty}{start};\nfn {function}() -> {ty} {{ return {var}; }}\n"));
}

/// The host for `reflection`, or an error naming every read Bevy cannot provide.
pub fn host(reflection: &Reflection) -> Result<Host, ShadewireError> {
    let context = &reflection.context;
    let mut declarations = String::from(HELPERS);
    declarations.push('\n');
    let mut statements = String::from(FRAME);
    let mut unsupported = Vec::new();
    for entry in context.varyings.iter().chain(&context.derived).chain(&context.globals) {
        match source(&entry.name, entry.ty) {
            Some(expr) => {
                let function = format!("{HOST_PREFIX}{}", entry.name);
                read(&mut declarations, &function, entry.ty, None);
                statements.push_str(&format!("    shadewire_in_{} = {expr};\n", entry.name));
            }
            None => unsupported.push(format!("{HOST_PREFIX}{}", entry.name)),
        }
    }
    for g in &context.geomprops {
        read(&mut declarations, &g.function, g.ty, g.default.as_ref().filter(|d| d.ty() == g.ty));
    }
    for t in &context.transforms {
        match transform_body(t) {
            Some(body) => {
                declarations.push_str(&format!("fn {HOST_PREFIX}{t}(v: vec3<f32>) -> vec3<f32> {{ return {body}; }}\n"))
            }
            None => unsupported.push(format!("{HOST_PREFIX}{t}")),
        }
    }
    if !unsupported.is_empty() {
        return Err(ShadewireError::UnsupportedHost { names: unsupported });
    }
    Ok(Host { declarations, statements })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shadewire::ContextEntry;
    use shadewire::reflection::{ContextReflection, GeomPropReflection};

    fn reflection(context: ContextReflection) -> Reflection {
        Reflection {
            shadewire: String::new(),
            source: String::new(),
            mtlx_version: String::new(),
            model: "gltf_pbr".into(),
            structure_hash: String::new(),
            params: Default::default(),
            uniforms: Vec::new(),
            uniform_block_size: 0,
            uniform_binding: None,
            group: 3,
            textures: Vec::new(),
            context,
            displacement: None,
            approximations: Vec::new(),
            closure: None,
        }
    }

    fn empty() -> ContextReflection {
        ContextReflection {
            varyings: Vec::new(),
            derived: Vec::new(),
            globals: Vec::new(),
            geomprops: Vec::new(),
            transforms: Vec::new(),
        }
    }

    fn entry(name: &str, ty: Type) -> ContextEntry {
        ContextEntry { name: name.into(), ty }
    }

    #[test]
    fn texcoord_flips_v() {
        let mut c = empty();
        c.varyings = vec![entry("texcoord_0", Type::Vector2), entry("texcoord_1", Type::Vector3)];
        let h = host(&reflection(c)).unwrap();
        assert!(h.statements.contains("shadewire_in_texcoord_0 = vec2<f32>(shadewire_uv0.x, 1.0 - shadewire_uv0.y);"));
        assert!(
            h.statements.contains("shadewire_in_texcoord_1 = vec3<f32>(shadewire_uv1.x, 1.0 - shadewire_uv1.y, 0.0);")
        );
        assert!(
            h.declarations.contains("fn shadewire_host_texcoord_0() -> vec2<f32> { return shadewire_in_texcoord_0; }")
        );
    }

    #[test]
    fn spaces_time_and_view_direction() {
        let mut c = empty();
        c.varyings = vec![entry("position_object", Type::Vector3), entry("normal_world", Type::Vector3)];
        c.derived = vec![entry("view_direction_world", Type::Vector3)];
        c.globals = vec![entry("frame", Type::Float), entry("time", Type::Float)];
        let h = host(&reflection(c)).unwrap();
        assert!(h.statements.contains(
            "shadewire_in_position_object = (shadewire_local_from_world * vec4<f32>(in.world_position.xyz, 1.0)).xyz;"
        ));
        assert!(h.statements.contains("shadewire_in_normal_world = shadewire_n;"));
        assert!(h.statements.contains("shadewire_in_view_direction_world = -pbr_input.V;"));
        assert!(h.statements.contains("shadewire_in_time = globals.time;"));
        assert!(h.statements.contains("shadewire_in_frame = f32(globals.frame_count);"));
    }

    #[test]
    fn geomprops_take_their_default() {
        let mut c = empty();
        c.geomprops = vec![GeomPropReflection {
            name: "tint".into(),
            ty: Type::Color3,
            function: "shadewire_host_geomprop_tint".into(),
            default: Some(Value::Color3([1.0, 0.5, 0.25])),
        }];
        let h = host(&reflection(c)).unwrap();
        assert!(
            h.declarations.contains("var<private> shadewire_in_geomprop_tint: vec3<f32> = vec3<f32>(1.0, 0.5, 0.25);")
        );
        assert!(h.declarations.contains("fn shadewire_host_geomprop_tint() -> vec3<f32>"));
    }

    #[test]
    fn transforms_use_the_mesh_matrices() {
        let mut c = empty();
        c.transforms = vec![
            "transform_point_model_to_world".into(),
            "transform_normal_world_to_object".into(),
            "transform_vector_object_to_model".into(),
        ];
        let h = host(&reflection(c)).unwrap();
        let d = &h.declarations;
        assert!(d.contains("fn shadewire_host_transform_point_model_to_world(v: vec3<f32>) -> vec3<f32> { return (shadewire_world_from_local * vec4<f32>(v, 1.0)).xyz; }"), "{d}");
        assert!(d.contains("transform_normal_world_to_object(v: vec3<f32>) -> vec3<f32> { return v * shadewire_mat3(shadewire_world_from_local); }"), "{d}");
        assert!(d.contains("transform_vector_object_to_model(v: vec3<f32>) -> vec3<f32> { return v; }"), "{d}");
    }

    #[test]
    fn back_faces_of_double_sided_materials_flip_the_whole_frame() {
        let h = host(&reflection(empty())).unwrap();
        let s = &h.statements;
        assert!(s.contains("winding_corrected_front_facing(pbr_input.flags, is_front)"), "{s}");
        assert!(s.contains("(pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT) != 0u"), "{s}");
        assert!(s.contains("let shadewire_n0 = normalize(in.world_normal);"), "{s}");
        assert!(s.contains("let shadewire_n = shadewire_side * shadewire_n0;"), "{s}");
        assert!(s.contains("let shadewire_t = shadewire_side * shadewire_t0;"), "{s}");
        assert!(s.contains("let shadewire_b = shadewire_side * "), "{s}");
        assert!(!s.contains("pbr_input.world_normal"), "{s}");
    }

    #[test]
    fn screen_space_tangent_takes_its_handedness_from_the_uvs() {
        let h = host(&reflection(empty())).unwrap();
        assert!(h.declarations.contains("let w = select(1.0, -1.0, dot(cross(n, t), b) < 0.0);"));
        assert!(h.statements.contains(
            "shadewire_screen_tangent(shadewire_n0, in.world_position.xyz, vec2<f32>(shadewire_uv0.x, 1.0 - shadewire_uv0.y))"
        ));
    }

    #[test]
    fn unknown_reads_are_named() {
        let mut c = empty();
        c.varyings = vec![entry("texcoord_2", Type::Vector2), entry("geomcolor_1", Type::Color3)];
        c.transforms = vec!["transform_point_camera_to_world".into()];
        let err = host(&reflection(c)).err().unwrap();
        assert_eq!(
            err.to_string(),
            "host functions Bevy does not provide: shadewire_host_texcoord_2, shadewire_host_geomcolor_1, \
             shadewire_host_transform_point_camera_to_world"
        );
    }
}
