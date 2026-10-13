//! Shading a closure tree with MaterialX's reference lobes in Godot. `light()` runs once per light and replaces
//! Godot's direct lighting: it rebuilds the surface `fragment()` computed and adds `shadewire_eval_bsdf`'s response
//! to `SPECULAR_LIGHT`, which Godot adds as is (it multiplies `DIFFUSE_LIGHT` by `ALBEDO`). `LIGHT_COLOR` is
//! color × energy × π and a MaterialX response carries the lobe's 1/π (Lambert is `albedo · cos / π`), so a white
//! Lambert lobe lights as Godot's own Lambert does.
//!
//! Ambient light and reflection probes still go through Godot's model: `fragment()` sets `ALBEDO` to the closure
//! tree's albedo (its response to a uniform white environment, `shadewire_eval_albedo`), with no metallic and no
//! specular, so the environment lights the surface as a diffuse one of that albedo. `EMISSION` is
//! `shadewire_eval_emission`. The directions are world-space, like the surface's normals and tangents.

use std::collections::HashMap;

use shadewire::reflection::live::live_nodes;
use shadewire::shader::names::struct_name;
use shadewire::{Dag, NodeId, NodeKind, Reflection, Type, Value};

use crate::names::{SURFACE, member};
use crate::types::{cast, gd_type, is_constant_literal, literal};

/// What ambient light and reflections lose: the specular part of the closure tree.
pub const AMBIENT: &str = "godot: ambient light and reflections see the closure tree's albedo as diffuse (no specular)";

/// Where `light()` finds a surface field: a constant written in, a uniform (a global), or the value `fragment()`
/// computed, passed through a varying.
enum Source {
    Literal(String),
    Uniform(String),
    Passed(usize),
}

/// How the surface reaches `light()`. Godot allows 32 varyings, so only the fields that vary per fragment travel,
/// each distinct DAG value once, packed back to back in `vec4`s (booleans and integers as floats).
pub struct Plan {
    sources: Vec<Source>,
    /// The passed values: the field `fragment()` reads each from, and its type.
    passed: Vec<(String, Type)>,
}

impl Plan {
    pub fn new(dag: &Dag, reflection: &Reflection, uniform_names: &[String]) -> Plan {
        let live = live_nodes(dag);
        let mut uniform_of: HashMap<NodeId, usize> = HashMap::new();
        for (i, n) in dag.nodes.iter().enumerate() {
            if live[i] && matches!(n.kind, NodeKind::Uniform { .. }) {
                let next = uniform_of.len();
                uniform_of.insert(NodeId(i as u32), next);
            }
        }
        let mut sources = Vec::new();
        let mut passed: Vec<(String, Type)> = Vec::new();
        let mut passed_of: HashMap<(NodeId, Type), usize> = HashMap::new();
        for f in &dag.surface.fields {
            let node = dag.node(f.node);
            let constant =
                reflection.params.get(&f.name).filter(|p| p.kind == "constant").and_then(|p| p.value.clone());
            let source = if let Some(value) = constant.filter(is_constant_literal) {
                Source::Literal(cast(&literal(&value), value.ty(), f.ty))
            } else if let Some(&i) = uniform_of.get(&f.node) {
                Source::Uniform(cast(&uniform_names[i], node.ty, f.ty))
            } else {
                let next = passed.len();
                let index = *passed_of.entry((f.node, f.ty)).or_insert(next);
                if index == next {
                    passed.push((f.name.clone(), f.ty));
                }
                Source::Passed(index)
            };
            sources.push(source);
        }
        Plan { sources, passed }
    }

    /// Each passed value's components as `(slot, lane)`, and the slot count.
    fn layout(&self) -> (Vec<Vec<(usize, char)>>, usize) {
        let mut at = 0;
        let mut out = Vec::new();
        for (_, ty) in &self.passed {
            let n = ty.component_count();
            out.push((at..at + n).map(|k| (k / 4, b"xyzw"[k % 4] as char)).collect());
            at += n;
        }
        (out, at.div_ceil(4))
    }

    /// How many `vec4` varyings the surface takes.
    pub fn varying_count(&self) -> usize {
        self.layout().1
    }

    /// The varyings carrying the surface, `fragment()` to `light()`.
    pub fn varyings(&self) -> String {
        (0..self.varying_count()).map(|i| format!("varying vec4 {};\n", slot(i))).collect()
    }

    /// The lines `fragment()` runs after evaluating the surface into `s`.
    pub fn fragment_lines(&self, emission_scale: f32) -> Vec<String> {
        let mut out = directions().to_vec();
        out.push(format!("ALBEDO = shadewire_eval_albedo({SURFACE}, shadewire_v, shadewire_n);"));
        out.push("METALLIC = 0.0;".into());
        out.push("SPECULAR = 0.0;".into());
        out.push("ROUGHNESS = 1.0;".into());
        let scale = if emission_scale == 1.0 {
            String::new()
        } else {
            format!(" * {}", literal(&Value::Float(emission_scale)))
        };
        out.push(format!("EMISSION = shadewire_eval_emission({SURFACE}, shadewire_v, shadewire_n){scale};"));
        let mut components: Vec<String> = Vec::new();
        for (field, ty) in &self.passed {
            let value = format!("{SURFACE}.{}", member(field));
            match ty {
                Type::Boolean => components.push(format!("({value} ? 1.0 : 0.0)")),
                Type::Integer => components.push(format!("float({value})")),
                Type::Float => components.push(value),
                ty => components.extend((0..ty.component_count()).map(|k| format!("{value}.{}", &"xyzw"[k..k + 1]))),
            }
        }
        components.resize(self.varying_count() * 4, "0.0".into());
        for (i, c) in components.chunks(4).enumerate() {
            out.push(format!("{} = vec4({});", slot(i), c.join(", ")));
        }
        out
    }

    /// `void light()`: the surface rebuilt, its BSDF for this light.
    pub fn light(&self, dag: &Dag) -> String {
        let model = struct_name(&dag.surface.model);
        let (lanes, _) = self.layout();
        let fields: Vec<String> = self
            .sources
            .iter()
            .map(|source| match source {
                Source::Literal(v) | Source::Uniform(v) => v.clone(),
                Source::Passed(i) => {
                    let parts: Vec<String> = lanes[*i].iter().map(|(s, lane)| format!("{}.{lane}", slot(*s))).collect();
                    match self.passed[*i].1 {
                        Type::Boolean => format!("{} > 0.5", parts[0]),
                        Type::Integer => format!("int({})", parts[0]),
                        Type::Float => parts[0].clone(),
                        ty => format!("{}({})", gd_type(ty), parts.join(", ")),
                    }
                }
            })
            .collect();
        let mut out = String::from("void light() {\n");
        out.push_str(&format!("    {model} {SURFACE} = {model}({});\n", fields.join(", ")));
        for line in directions() {
            out.push_str(&format!("    {line}\n"));
        }
        out.push_str("    vec3 shadewire_l = normalize((INV_VIEW_MATRIX * vec4(LIGHT, 0.0)).xyz);\n");
        out.push_str(&format!(
            "    SPECULAR_LIGHT += shadewire_eval_bsdf({SURFACE}, shadewire_l, shadewire_v, shadewire_n).response * LIGHT_COLOR * ATTENUATION;\n"
        ));
        out.push_str("}\n");
        out
    }
}

fn slot(i: usize) -> String {
    format!("shadewire_l{i}")
}

/// World-space shading normal and direction to the viewer, from the stage's view-space built-ins.
fn directions() -> [String; 2] {
    [
        "vec3 shadewire_n = normalize((INV_VIEW_MATRIX * vec4(NORMAL, 0.0)).xyz);".into(),
        "vec3 shadewire_v = normalize((INV_VIEW_MATRIX * vec4(VIEW, 0.0)).xyz);".into(),
    ]
}
