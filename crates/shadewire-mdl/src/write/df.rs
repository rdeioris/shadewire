//! The closure graph as `::df` calls: one `bsdf` or `edf` binding per node in the material's `let`, after
//! MaterialX's own MDL (`pbrlib_1_6.mdl`). A layer passes its base into the lobes above that take one, so pbrlib's
//! `layer` of a dielectric over a diffuse is one `fresnel_layer`.

use std::collections::HashMap;

use shadewire::shader::names::ident;
use shadewire::{ClosureGraph, ClosureId, ClosureNode, Dag, NodeKind, Type};

/// The local holding the surface fields.
const SURFACE: &str = "s";

pub struct Closures<'a> {
    dag: &'a Dag,
    graph: &'a ClosureGraph,
    /// The parameter a uniform's name became, for the uniform fields the material struct needs uniform.
    uniform_param: &'a dyn Fn(&str) -> Option<String>,
    /// The `let` lines, in order.
    pub lines: Vec<String>,
    pub notes: Vec<String>,
    /// The bindings made so far, by node, for nodes written without a base.
    bound: HashMap<ClosureId, String>,
    /// The ior of a transmitting lobe, for the material's `ior`.
    ior: Option<String>,
}

impl<'a> Closures<'a> {
    pub fn new(dag: &'a Dag, graph: &'a ClosureGraph, uniform_param: &'a dyn Fn(&str) -> Option<String>) -> Self {
        Closures { dag, graph, uniform_param, lines: Vec::new(), notes: Vec::new(), bound: HashMap::new(), ior: None }
    }

    fn field_ty(&self, field: &str) -> Option<Type> {
        self.dag.surface.fields.iter().find(|f| f.name == field).map(|f| f.ty)
    }

    /// Whether `field` is a constant of zero (or missing).
    fn zero(&self, field: Option<&String>) -> bool {
        let Some(field) = field else { return true };
        self.dag.surface.fields.iter().find(|f| &f.name == field).is_none_or(
            |f| matches!(&self.dag.node(f.node).kind, NodeKind::Const(v) if v.floats().iter().all(|x| *x == 0.0)),
        )
    }

    /// Field `field` read from the surface struct.
    fn read(field: &str) -> String {
        format!("{SURFACE}.{}", ident(field))
    }

    /// Input `input` of a lobe as a float (`default` when the lobe has none).
    fn float(inputs: &std::collections::BTreeMap<String, String>, input: &str, default: &str) -> String {
        inputs.get(input).map_or(default.to_string(), |f| Self::read(f))
    }

    /// Input `input` as a color.
    fn color(inputs: &std::collections::BTreeMap<String, String>, input: &str, default: &str) -> String {
        inputs.get(input).map_or(default.to_string(), |f| format!("color({})", Self::read(f)))
    }

    /// A GGX lobe over the lobe's roughness and tangent.
    fn ggx(inputs: &std::collections::BTreeMap<String, String>, tint: &str, mode: &str) -> String {
        let (u, v) = match inputs.get("roughness") {
            Some(f) => (format!("{}.x", Self::read(f)), format!("{}.y", Self::read(f))),
            None => ("0.05".to_string(), "0.05".to_string()),
        };
        let tangent = inputs.get("tangent").map_or(String::new(), |f| format!(", tangent_u: {}", Self::read(f)));
        format!(
            "df::microfacet_ggx_smith_bsdf(roughness_u: {u}, roughness_v: {v}, tint: {tint}{tangent}, mode: {mode})"
        )
    }

    fn normal(inputs: &std::collections::BTreeMap<String, String>) -> String {
        inputs.get("normal").map_or(String::new(), |f| format!(", normal: {}", Self::read(f)))
    }

    /// `bsdf` wrapped in a thin film when the lobe has one.
    fn thin_film(&self, inputs: &std::collections::BTreeMap<String, String>, bsdf: String) -> String {
        if self.zero(inputs.get("thinfilm_thickness")) {
            return bsdf;
        }
        let thickness = Self::float(inputs, "thinfilm_thickness", "0.0");
        let ior = Self::float(inputs, "thinfilm_ior", "1.5");
        format!("df::thin_film(thickness: {thickness}, ior: color({ior}), base: {bsdf})")
    }

    /// A binding of `expr` of `ty` (`bsdf`, `edf`), returning its name.
    fn bind(&mut self, ty: &str, expr: String) -> String {
        let name = format!("c{}", self.lines.len());
        self.lines.push(format!("{ty} {name} = {expr};"));
        name
    }

    /// The BSDF of node `id`, with `base` under it where the node is a lobe that layers over one.
    pub fn bsdf(&mut self, id: ClosureId, base: Option<&str>) -> String {
        if base.is_none()
            && let Some(name) = self.bound.get(&id)
        {
            return name.clone();
        }
        let expr = match &self.graph.nodes[id as usize] {
            ClosureNode::Lobe { kind, inputs, settings, .. } => self.lobe(kind, inputs, settings, base),
            ClosureNode::Layer { top, base: under } => {
                let under = self.bsdf(*under, None);
                if self.takes_base(*top) {
                    return self.bsdf(*top, Some(&under));
                }
                let top = self.bsdf(*top, None);
                format!("df::weighted_layer(weight: 1.0, layer: {top}, base: {under})")
            }
            ClosureNode::Mix { fg, bg, mix } => match self.graph.nodes[*fg as usize] {
                // The reader's weighted layer, `mix(layer(top, base), base, w)`.
                ClosureNode::Layer { top, base } if base == *bg => {
                    let (top, under) = (self.bsdf(top, None), self.bsdf(base, None));
                    format!("df::weighted_layer(weight: {}, layer: {top}, base: {under})", Self::read(mix))
                }
                _ => {
                    let (fg, bg, w) = (self.bsdf(*fg, None), self.bsdf(*bg, None), Self::read(mix));
                    format!(
                        "df::normalized_mix(df::bsdf_component[](df::bsdf_component({w}, {fg}), df::bsdf_component(1.0 - {w}, {bg})))"
                    )
                }
            },
            ClosureNode::Add { in1, in2 } => {
                let (a, b) = (self.bsdf(*in1, None), self.bsdf(*in2, None));
                format!(
                    "df::unbounded_mix(df::bsdf_component[](df::bsdf_component(1.0, {a}), df::bsdf_component(1.0, {b})))"
                )
            }
            ClosureNode::Multiply { in1, in2 } => {
                let a = self.bsdf(*in1, None);
                format!("df::tint(color({}), {a})", Self::read(in2))
            }
        };
        let name = self.bind("bsdf", expr);
        if base.is_none() {
            self.bound.insert(id, name.clone());
        }
        name
    }

    /// Whether node `id` is a lobe that takes the base of a layer.
    fn takes_base(&self, id: ClosureId) -> bool {
        let ClosureNode::Lobe { kind, settings, .. } = &self.graph.nodes[id as usize] else { return false };
        let mode = settings.get("scatter_mode").map_or("R", String::as_str);
        match kind.as_str() {
            "dielectric_bsdf" => mode == "R",
            "generalized_schlick_bsdf" => mode == "R",
            "sheen_bsdf" => true,
            _ => false,
        }
    }

    fn lobe(
        &mut self,
        kind: &str,
        inputs: &std::collections::BTreeMap<String, String>,
        settings: &std::collections::BTreeMap<String, String>,
        base: Option<&str>,
    ) -> String {
        let base = base.unwrap_or("bsdf()");
        let weight = Self::float(inputs, "weight", "1.0");
        let normal = Self::normal(inputs);
        let mode = settings.get("scatter_mode").map_or("R", String::as_str);
        let scatter = match mode {
            "T" => "df::scatter_transmit",
            "RT" => "df::scatter_reflect_transmit",
            _ => "df::scatter_reflect",
        };
        let weighted = |layer: String| format!("df::weighted_layer(weight: {weight}, layer: {layer}{normal})");
        match kind {
            "oren_nayar_diffuse_bsdf" | "burley_diffuse_bsdf" | "subsurface_bsdf" => {
                if kind == "subsurface_bsdf" {
                    self.notes.push("mdl: subsurface_bsdf written as diffuse reflection".to_string());
                }
                let roughness = Self::float(inputs, "roughness", "0.0");
                let tint = Self::color(inputs, "color", "color(0.18)");
                weighted(format!("df::diffuse_reflection_bsdf(tint: {tint}, roughness: {roughness})"))
            }
            "translucent_bsdf" => {
                weighted(format!("df::diffuse_transmission_bsdf(tint: {})", Self::color(inputs, "color", "color(1.0)")))
            }
            "dielectric_bsdf" => {
                let tint = Self::color(inputs, "tint", "color(1.0)");
                let ior = Self::float(inputs, "ior", "1.5");
                if mode == "R" {
                    let layer = Self::ggx(inputs, &tint, scatter);
                    let bsdf = format!(
                        "df::fresnel_layer(ior: {ior}, weight: {weight}, layer: {layer}, base: {base}{normal})"
                    );
                    self.thin_film(inputs, bsdf)
                } else {
                    // Transmission refracts by the material's ior.
                    self.ior.get_or_insert_with(|| format!("color({ior})"));
                    let layer = self.thin_film(inputs, Self::ggx(inputs, &tint, scatter));
                    weighted(layer)
                }
            }
            "conductor_bsdf" => {
                let ior = Self::color(inputs, "ior", "color(0.183, 0.421, 1.373)");
                let k = Self::color(inputs, "extinction", "color(3.424, 2.346, 1.77)");
                let ggx = Self::ggx(inputs, "color(1.0)", "df::scatter_reflect");
                let factor = format!("df::fresnel_factor(ior: {ior}, extinction_coefficient: {k}, base: {ggx})");
                weighted(self.thin_film(inputs, factor))
            }
            "generalized_schlick_bsdf" => {
                let c0 = Self::color(inputs, "color0", "color(1.0)");
                let c82 = Self::color(inputs, "color82", "color(1.0)");
                let c90 = Self::color(inputs, "color90", "color(1.0)");
                let exponent = Self::float(inputs, "exponent", "5.0");
                // MDL 1.7's curve has no F82 term: the grazing color takes it, as MaterialX's MDL does.
                let grazing = format!("{c82} * {c90}");
                let curve = |layer: String, under: String, w: &str| {
                    format!(
                        "df::color_custom_curve_layer(normal_reflectivity: {c0}, grazing_reflectivity: {grazing}, exponent: {exponent}, weight: color({w}), layer: {layer}, base: {under}{normal})"
                    )
                };
                match mode {
                    "T" => weighted(curve("bsdf()".to_string(), Self::ggx(inputs, "color(1.0)", scatter), "1.0")),
                    "RT" => {
                        let r = Self::ggx(inputs, "color(1.0)", "df::scatter_reflect");
                        let t = Self::ggx(inputs, "color(1.0)", "df::scatter_transmit");
                        self.thin_film(inputs, curve(r, t, &weight))
                    }
                    _ => {
                        let r = Self::ggx(inputs, "color(1.0)", "df::scatter_reflect");
                        self.thin_film(inputs, curve(r, base.to_string(), &weight))
                    }
                }
            }
            "sheen_bsdf" => {
                let roughness = Self::float(inputs, "roughness", "0.3");
                let tint = Self::color(inputs, "color", "color(1.0)");
                // A black multiscatter: the base takes what the sheen leaves, through the weighted layer.
                let sheen = format!("df::sheen_bsdf(roughness: {roughness}, tint: {tint}, multiscatter: bsdf())");
                format!("df::weighted_layer(weight: {weight}, layer: {sheen}, base: {base}{normal})")
            }
            other => {
                self.notes.push(format!("mdl: {other} scatters nothing"));
                "bsdf()".to_string()
            }
        }
    }

    /// The EDF of node `id` (MaterialX's radiance as MDL's exitance: the intensity is π).
    pub fn edf(&mut self, id: ClosureId) -> String {
        if let Some(name) = self.bound.get(&id) {
            return name.clone();
        }
        let expr = match &self.graph.nodes[id as usize] {
            ClosureNode::Lobe { kind, inputs, closures, .. } => match kind.as_str() {
                "uniform_edf" => format!("df::tint({}, df::diffuse_edf())", Self::color(inputs, "color", "color(1.0)")),
                "conical_edf" | "measured_edf" => {
                    self.notes.push(format!("mdl: {kind} written as uniform emission"));
                    format!("df::tint({}, df::diffuse_edf())", Self::color(inputs, "color", "color(1.0)"))
                }
                other => {
                    self.notes.push(format!("mdl: {other} written as its base"));
                    match closures.get("base") {
                        Some(b) => self.edf(*b),
                        None => "edf()".to_string(),
                    }
                }
            },
            ClosureNode::Mix { fg, bg, mix } => {
                let (fg, bg, w) = (self.edf(*fg), self.edf(*bg), Self::read(mix));
                format!(
                    "df::normalized_mix(df::edf_component[](df::edf_component({w}, {fg}), df::edf_component(1.0 - {w}, {bg})))"
                )
            }
            ClosureNode::Add { in1, in2 } => {
                let (a, b) = (self.edf(*in1), self.edf(*in2));
                format!(
                    "df::unbounded_mix(df::edf_component[](df::edf_component(1.0, {a}), df::edf_component(1.0, {b})))"
                )
            }
            ClosureNode::Multiply { in1, in2 } => {
                let a = self.edf(*in1);
                format!("df::tint(color({}), {a})", Self::read(in2))
            }
            ClosureNode::Layer { top, .. } => self.edf(*top),
        };
        let name = self.bind("edf", expr);
        self.bound.insert(id, name.clone());
        name
    }

    /// `material(...)` over the graph's roots.
    pub fn material(&mut self) -> String {
        let mut fields = Vec::new();
        if let Some(t) = self.dag.surface.fields.iter().find(|f| f.name == "thin_walled") {
            // `thin_walled` must be uniform: a constant or a parameter.
            match &self.dag.node(t.node).kind {
                NodeKind::Const(v) if v.as_bool() => fields.push("thin_walled: true".to_string()),
                NodeKind::Const(_) => {}
                NodeKind::Uniform { name, .. } => match (self.uniform_param)(name) {
                    Some(p) => fields.push(format!("thin_walled: {p}")),
                    None => self.notes.push("mdl: thin_walled not uniform, false".to_string()),
                },
                _ => self.notes.push("mdl: thin_walled computed at run time, false".to_string()),
            }
        }
        let bsdf = self.graph.bsdf.map(|b| self.bsdf(b, None));
        let edf = self.graph.edf.map(|e| self.edf(e));
        if let Some(ior) = &self.ior {
            fields.push(format!("ior: {ior}"));
        }
        let mut surface = Vec::new();
        if let Some(b) = bsdf {
            surface.push(format!("scattering: {b}"));
        }
        if let Some(e) = edf {
            surface.push(format!("emission: material_emission(emission: {e}, intensity: color(math::PI))"));
        }
        if !surface.is_empty() {
            fields.push(format!("surface: material_surface({})", surface.join(", ")));
        }
        if let Some(o) = &self.graph.opacity
            && self.field_ty(o).is_some()
        {
            fields.push(format!("geometry: material_geometry(cutout_opacity: {})", Self::read(o)));
        }
        format!("material({})", fields.join(", "))
    }
}
