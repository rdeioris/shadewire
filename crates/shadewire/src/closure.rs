//! The closure tree of a `surface` node: lobes (MaterialX BSDF and EDF leaf nodes) combined by `layer`,
//! `mix`, `add` and `multiply`.
//!
//! The tree's shape is fixed per material; every value it reads (a lobe input, a mix weight, the opacity)
//! is a surface field, computed by the DAG like any other model's field. A back end may evaluate the tree
//! (the WGSL one writes `shadewire_eval_bsdf` and friends from MaterialX's reference lobes); a host with
//! other needs, such as a path tracer sampling directions, walks it itself.

use std::collections::BTreeMap;

use serde::Serialize;

/// Index of a node in `ClosureGraph::nodes`.
pub type ClosureId = u32;

/// Whether a closure scatters light at the surface (`bsdf`), emits it (`edf`), or scatters and absorbs it
/// inside the volume behind the surface (`vdf`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClosureType {
    Bsdf,
    Edf,
    Vdf,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ClosureNode {
    /// A leaf: `kind` is the MaterialX node category (`oren_nayar_diffuse_bsdf`, `uniform_edf`...).
    /// `inputs` maps each value input to the surface field holding it; `settings` holds the string inputs
    /// (`scatter_mode`), which are fixed per material. `closures` holds the closure inputs of the few lobes
    /// that modify another closure (`generalized_schlick_edf` shapes the emission of its `base`).
    Lobe {
        kind: String,
        #[serde(rename = "type")]
        ty: ClosureType,
        inputs: BTreeMap<String, String>,
        settings: BTreeMap<String, String>,
        #[serde(skip_serializing_if = "BTreeMap::is_empty")]
        closures: BTreeMap<String, ClosureId>,
    },
    /// `top` over `base`: the base receives what the top transmits.
    Layer {
        top: ClosureId,
        base: ClosureId,
    },
    /// `mix(bg, fg, mix)`: `mix` names a float field.
    Mix {
        fg: ClosureId,
        bg: ClosureId,
        mix: String,
    },
    Add {
        in1: ClosureId,
        in2: ClosureId,
    },
    /// `in1` scaled by `in2`, a float or color3 field.
    Multiply {
        in1: ClosureId,
        in2: String,
    },
}

impl ClosureNode {
    /// The closure nodes this one reads.
    pub fn children(&self) -> Vec<ClosureId> {
        match self {
            ClosureNode::Lobe { closures, .. } => closures.values().copied().collect(),
            ClosureNode::Layer { top, base } => vec![*top, *base],
            ClosureNode::Mix { fg, bg, .. } => vec![*fg, *bg],
            ClosureNode::Add { in1, in2 } => vec![*in1, *in2],
            ClosureNode::Multiply { in1, .. } => vec![*in1],
        }
    }

    /// The surface fields this node reads.
    pub fn fields(&self) -> Vec<&str> {
        match self {
            ClosureNode::Lobe { inputs, .. } => inputs.values().map(String::as_str).collect(),
            ClosureNode::Mix { mix, .. } => vec![mix.as_str()],
            ClosureNode::Multiply { in2, .. } => vec![in2.as_str()],
            ClosureNode::Layer { .. } | ClosureNode::Add { .. } => Vec::new(),
        }
    }
}

/// A `surface` node's closures. Children come before their parents in `nodes`, so a host may evaluate
/// the nodes in order. A shared sub-closure is one node.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ClosureGraph {
    pub nodes: Vec<ClosureNode>,
    /// The scattering tree; `None` scatters nothing (a pure emitter or a black surface).
    pub bsdf: Option<ClosureId>,
    /// The emission tree; `None` emits nothing.
    pub edf: Option<ClosureId>,
    /// The float field holding the cutout opacity; `None` is opaque.
    pub opacity: Option<String>,
}

impl ClosureGraph {
    pub fn add(&mut self, node: ClosureNode) -> ClosureId {
        for child in node.children() {
            assert!((child as usize) < self.nodes.len(), "closure child {child} added after its parent");
        }
        self.nodes.push(node);
        (self.nodes.len() - 1) as ClosureId
    }

    /// One line per node, then the roots: the closure part of `Dag::dump`.
    pub fn dump(&self) -> String {
        let mut out = String::new();
        for (index, node) in self.nodes.iter().enumerate() {
            let body = match node {
                ClosureNode::Lobe { kind, inputs, settings, closures, .. } => {
                    let mut parts: Vec<String> = closures.iter().map(|(k, c)| format!("{k}=c{c}")).collect();
                    parts.extend(inputs.iter().map(|(k, v)| format!("{k}={v}")));
                    parts.extend(settings.iter().map(|(k, v)| format!("{k}=\"{v}\"")));
                    format!("{kind}({})", parts.join(", "))
                }
                ClosureNode::Layer { top, base } => format!("layer(top=c{top}, base=c{base})"),
                ClosureNode::Mix { fg, bg, mix } => format!("mix(fg=c{fg}, bg=c{bg}, mix={mix})"),
                ClosureNode::Add { in1, in2 } => format!("add(c{in1}, c{in2})"),
                ClosureNode::Multiply { in1, in2 } => format!("multiply(c{in1}, {in2})"),
            };
            out.push_str(&format!("c{index} = {body}\n"));
        }
        let root = |r: Option<ClosureId>| r.map_or("none".to_string(), |c| format!("c{c}"));
        out.push_str(&format!("bsdf = {}\nedf = {}\n", root(self.bsdf), root(self.edf)));
        out.push_str(&format!("opacity = {}\n", self.opacity.as_deref().unwrap_or("none")));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lobe(kind: &str, field: &str) -> ClosureNode {
        ClosureNode::Lobe {
            kind: kind.into(),
            ty: ClosureType::Bsdf,
            inputs: BTreeMap::from([("color".to_string(), field.to_string())]),
            settings: BTreeMap::new(),
            closures: BTreeMap::new(),
        }
    }

    #[test]
    fn a_layer_dumps_its_children_first() {
        let mut g = ClosureGraph::default();
        let top = g.add(lobe("dielectric_bsdf", "c0_tint"));
        let base = g.add(lobe("oren_nayar_diffuse_bsdf", "c1_color"));
        g.bsdf = Some(g.add(ClosureNode::Layer { top, base }));
        assert_eq!(
            g.dump(),
            "c0 = dielectric_bsdf(color=c0_tint)\nc1 = oren_nayar_diffuse_bsdf(color=c1_color)\n\
             c2 = layer(top=c0, base=c1)\nbsdf = c2\nedf = none\nopacity = none\n"
        );
    }

    #[test]
    #[should_panic(expected = "added after its parent")]
    fn children_must_exist_first() {
        let mut g = ClosureGraph::default();
        g.add(ClosureNode::Add { in1: 0, in2: 1 });
    }

    #[test]
    fn reflection_json_tags_each_node_by_op() {
        let mut g = ClosureGraph::default();
        let a = g.add(lobe("oren_nayar_diffuse_bsdf", "c0_color"));
        g.add(ClosureNode::Multiply { in1: a, in2: "m1_in2".into() });
        let json = serde_json::to_string(&g.nodes).unwrap();
        assert_eq!(
            json,
            r#"[{"op":"lobe","kind":"oren_nayar_diffuse_bsdf","type":"bsdf","inputs":{"color":"c0_color"},"settings":{}},{"op":"multiply","in1":0,"in2":"m1_in2"}]"#
        );
    }
}
