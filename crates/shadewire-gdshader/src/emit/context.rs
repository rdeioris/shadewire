//! The host side Godot needs generated: stage built-ins are visible only inside `vertex()`,
//! `fragment()` and `light()`, and there are no mutable globals, so `fragment()` fills a
//! `ShadewireContext` with every live read and passes it to `shadewire_eval_surface`.

use shadewire::reflection::live::live_nodes;
use shadewire::{ContextRead, Dag, HOST_PREFIX, NodeKind, Reflection, Space, Type, Value};

use super::names::{CONTEXT, CONTEXT_STRUCT};
use super::ops::{cast, needs_matrices};
use super::types::{gd_type, literal};

/// One context member: its name, its type and the `fragment()` expression that fills it.
pub struct Member {
    pub name: String,
    pub ty: &'static str,
    pub source: String,
}

/// What the context needs: its members in reflection order, the varyings `vertex()` writes
/// (name, type, model-space built-in) and the approximations Godot forces on the reads.
pub struct Context {
    pub members: Vec<Member>,
    pub varyings: Vec<(String, &'static str, &'static str)>,
    pub approximations: Vec<String>,
}

/// The view-space built-in of a direction read and its model-space twin in `vertex()`.
fn built_in(read: &ContextRead) -> Option<&'static str> {
    match read {
        ContextRead::Position(_) => Some("VERTEX"),
        ContextRead::Normal(_) => Some("NORMAL"),
        ContextRead::Tangent(_) => Some("TANGENT"),
        ContextRead::Bitangent(_) => Some("BINORMAL"),
        _ => None,
    }
}

const POSITION_WORLD: &str = "(INV_VIEW_MATRIX * vec4(VERTEX, 1.0)).xyz";

/// The live reads, one per reflection name, sorted by it (reflection's order).
fn reads(dag: &Dag) -> Vec<ContextRead> {
    let live = live_nodes(dag);
    let mut out: Vec<ContextRead> = Vec::new();
    for (i, node) in dag.nodes.iter().enumerate() {
        if let NodeKind::Context(read) = &node.kind {
            if live[i] && !out.iter().any(|r| r.reflection_name() == read.reflection_name()) {
                out.push(read.clone());
            }
        }
    }
    out.sort_by_key(|r| r.reflection_name());
    out
}

/// The member name of a read: the reflection name, or for a geomprop the sanitized and unique
/// name reflection gave its host function.
pub fn member_name(read: &ContextRead, reflection: &Reflection) -> String {
    match read {
        ContextRead::GeomProp { name, .. } => {
            let g = reflection.context.geomprops.iter().find(|g| g.name == *name).expect("live geomprop is reflected");
            g.function.strip_prefix(HOST_PREFIX).unwrap_or(&g.function).to_string()
        }
        other => other.reflection_name(),
    }
}

pub fn context(dag: &Dag, reflection: &Reflection) -> Context {
    let mut members = Vec::new();
    let mut varyings: Vec<(String, &'static str, &'static str)> = Vec::new();
    let mut approximations = Vec::new();
    for read in reads(dag) {
        let ty = read.ty();
        let name = member_name(&read, reflection);
        let zero = || literal(&Value::zero(ty));
        let source = match &read {
            ContextRead::Position(Space::World) => POSITION_WORLD.to_string(),
            ContextRead::Normal(Space::World)
            | ContextRead::Tangent(Space::World)
            | ContextRead::Bitangent(Space::World) => {
                format!("normalize((INV_VIEW_MATRIX * vec4({}, 0.0)).xyz)", built_in(&read).unwrap())
            }
            // Godot has one object space: `vertex()` sees VERTEX, NORMAL, TANGENT and BINORMAL in it.
            ContextRead::Position(_) | ContextRead::Normal(_) | ContextRead::Tangent(_) | ContextRead::Bitangent(_) => {
                let input = built_in(&read).unwrap();
                let kind = read.reflection_name();
                let kind = kind.rsplit_once('_').map_or(kind.as_str(), |(k, _)| k);
                let varying = format!("shadewire_v_{kind}_object");
                if !varyings.iter().any(|v| v.0 == varying) {
                    varyings.push((varying.clone(), "vec3", input));
                }
                if matches!(read, ContextRead::Position(_)) { varying } else { format!("normalize({varying})") }
            }
            ContextRead::TexCoord { index, ty } if *index < 2 => {
                let uv = if *index == 0 { "UV" } else { "UV2" };
                // Godot's UV has V down, MaterialX's V up.
                let st = format!("vec2({uv}.x, 1.0 - {uv}.y)");
                match ty.component_count() {
                    2 => st,
                    3 => format!("vec3({st}, 0.0)"),
                    _ => cast(&format!("{st}.x"), Type::Float, *ty),
                }
            }
            ContextRead::GeomColor { index: 0, ty } => match ty.component_count() {
                4 => "COLOR".to_string(),
                3 => "COLOR.rgb".to_string(),
                2 => "COLOR.rg".to_string(),
                _ => cast("COLOR.r", Type::Float, *ty),
            },
            ContextRead::TexCoord { .. } | ContextRead::GeomColor { .. } => {
                approximations.push(format!("godot: {} not available (zero)", read.reflection_name()));
                zero()
            }
            ContextRead::GeomProp { name: prop, .. } => {
                let g = reflection.context.geomprops.iter().find(|g| g.name == *prop).expect("reflected");
                match g.default.as_ref().filter(|d| d.ty() == ty) {
                    Some(d) => {
                        approximations.push(format!("godot: geomprop {prop} is its default"));
                        literal(d)
                    }
                    None => {
                        approximations.push(format!("godot: geomprop {prop} not available (zero)"));
                        zero()
                    }
                }
            }
            // MaterialX's view direction points from the camera to the surface.
            ContextRead::ViewDirection(Space::World) => format!("normalize({POSITION_WORLD} - CAMERA_POSITION_WORLD)"),
            ContextRead::ViewDirection(_) => {
                format!("normalize((inverse(MODEL_MATRIX) * vec4({POSITION_WORLD} - CAMERA_POSITION_WORLD, 0.0)).xyz)")
            }
            ContextRead::Time => "TIME".to_string(),
            ContextRead::Frame => {
                approximations.push("godot: frame is floor(TIME * 60.0)".to_string());
                "floor(TIME * 60.0)".to_string()
            }
        };
        members.push(Member { name, ty: gd_type(ty), source });
    }
    let transforms = dag
        .nodes
        .iter()
        .zip(live_nodes(dag))
        .any(|(n, live)| live && matches!(&n.kind, NodeKind::Op { op, .. } if needs_matrices(op)));
    if transforms {
        members.push(Member { name: "model_matrix".into(), ty: "mat4", source: "MODEL_MATRIX".into() });
        members.push(Member { name: "model_normal_matrix".into(), ty: "mat3", source: "MODEL_NORMAL_MATRIX".into() });
    }
    Context { members, varyings, approximations }
}

impl Context {
    /// `struct ShadewireContext { ... };`, empty when nothing is read.
    pub fn struct_text(&self) -> String {
        if self.members.is_empty() {
            return String::new();
        }
        let mut out = format!("struct {CONTEXT_STRUCT} {{\n");
        for m in &self.members {
            out.push_str(&format!("    {} {};\n", m.ty, m.name));
        }
        out.push_str("};\n");
        out
    }

    /// The parameter list of the entry function.
    pub fn parameter(&self) -> String {
        if self.members.is_empty() { String::new() } else { format!("{CONTEXT_STRUCT} {CONTEXT}") }
    }

    /// The `fragment()` lines that fill the context, and the argument to pass.
    pub fn fill(&self) -> (Vec<String>, String) {
        if self.members.is_empty() {
            return (Vec::new(), String::new());
        }
        let mut lines = vec![format!("{CONTEXT_STRUCT} {CONTEXT};")];
        for m in &self.members {
            lines.push(format!("{CONTEXT}.{} = {};", m.name, m.source));
        }
        (lines, CONTEXT.to_string())
    }

    /// The varying declarations and the `vertex()` that writes them; empty without varyings.
    pub fn vertex_text(&self) -> String {
        if self.varyings.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        for (name, ty, _) in &self.varyings {
            out.push_str(&format!("varying {ty} {name};\n"));
        }
        out.push_str("\nvoid vertex() {\n");
        for (name, _, input) in &self.varyings {
            out.push_str(&format!("    {name} = {input};\n"));
        }
        out.push_str("}\n");
        out
    }
}
