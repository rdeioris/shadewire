//! A material's graph flattened for lowering: groups inlined with prefixed names, reroutes followed, muted nodes
//! bypassed, so the lowerer sees plain nodes and links alone.

use std::collections::{HashMap, HashSet};

use serde_json::Value as Json;
use shadewire::Error;

use crate::document::{Document, Group, Link, Material, Node, Socket, at};

/// The kind of a synthetic node holding one value, on its output `Value`: what a muted node or a group input
/// hands its readers when nothing is linked to it.
pub const VALUE_KIND: &str = "shadewire.value";

const VALUE_OUTPUT: &str = "Value";
const FRAME: &str = "NodeFrame";
const REROUTE: &str = "NodeReroute";
const GROUP: &str = "ShaderNodeGroup";
const GROUP_INPUT: &str = "NodeGroupInput";
const GROUP_OUTPUT: &str = "NodeGroupOutput";

/// `(node name, socket identifier)`.
type Source = (String, String);

/// A node of the flattened graph: a plain Blender node or a synthetic value node, under its prefixed name.
#[derive(Clone, Debug)]
pub struct FlatNode {
    pub name: String,
    /// Blender's `bl_idname`, or [`VALUE_KIND`].
    pub kind: String,
    pub properties: serde_json::Map<String, Json>,
    pub inputs: Vec<Socket>,
    pub outputs: Vec<Socket>,
}

impl FlatNode {
    pub fn input(&self, id: &str) -> Option<&Socket> {
        self.inputs.iter().find(|s| s.identifier == id)
    }

    pub fn output(&self, id: &str) -> Option<&Socket> {
        self.outputs.iter().find(|s| s.identifier == id)
    }

    /// A string property (an enum identifier or a name), `None` when absent or not a string.
    pub fn text(&self, key: &str) -> Option<&str> {
        self.properties.get(key).and_then(Json::as_str)
    }

    pub fn flag(&self, key: &str) -> bool {
        self.properties.get(key).and_then(Json::as_bool).unwrap_or(false)
    }

    pub fn integer(&self, key: &str) -> Option<i64> {
        self.properties.get(key).and_then(Json::as_i64)
    }
}

/// A material's nodes and the links between them, with no groups, reroutes, frames or muted nodes left.
#[derive(Clone, Debug)]
pub struct Graph {
    pub nodes: Vec<FlatNode>,
    /// The material's active output node, as the document names it.
    pub output: Option<String>,
    /// `(to node, to input)` to `(from node, from output)`.
    links: HashMap<(String, String), Source>,
    by_name: HashMap<String, usize>,
}

impl Graph {
    /// The flattened graph of `material` and the warnings the flattening raised.
    pub fn flatten(doc: &Document, material: &Material) -> Result<(Graph, Vec<String>), Error> {
        let graph = Graph {
            nodes: Vec::new(),
            output: material.output.clone(),
            links: HashMap::new(),
            by_name: HashMap::new(),
        };
        let mut builder = Builder {
            doc,
            graph,
            warnings: Vec::new(),
            scopes: Vec::new(),
            instances: HashMap::new(),
            memo: HashMap::new(),
            active: HashSet::new(),
        };
        let root = builder.scope(&material.nodes, &material.links, String::new(), None)?;
        builder.populate(root)?;
        Ok((builder.graph, builder.warnings))
    }

    pub fn node(&self, name: &str) -> Option<&FlatNode> {
        self.by_name.get(name).map(|&i| &self.nodes[i])
    }

    /// The `(node, output)` feeding `node`'s `input`, `None` when the input is unlinked.
    pub fn source(&self, node: &str, input: &str) -> Option<(&str, &str)> {
        let from = self.links.get(&(node.to_string(), input.to_string()))?;
        Some((from.0.as_str(), from.1.as_str()))
    }
}

fn bad(message: String) -> Error {
    Error::BadValue { at: at("graph"), message }
}

fn is_structural(kind: &str) -> bool {
    matches!(kind, FRAME | REROUTE | GROUP | GROUP_INPUT | GROUP_OUTPUT)
}

/// The socket type Blender reports for a group interface socket (`NodeSocketFloatFactor` is a `VALUE`).
fn socket_kind(idname: &str) -> String {
    let name = idname.strip_prefix("NodeSocket").unwrap_or(idname);
    let kind = match name {
        n if n.starts_with("Float") => "VALUE",
        n if n.starts_with("Int") => "INT",
        "Bool" => "BOOLEAN",
        "Color" => "RGBA",
        "Rotation" => "ROTATION",
        "String" => "STRING",
        "Shader" => "SHADER",
        "Vector" | "Translation" | "Direction" | "Velocity" | "Acceleration" | "Euler" | "XYZ" => "VECTOR",
        other => return other.to_uppercase(),
    };
    kind.to_string()
}

/// One node list being flattened: the material's, or one inlined instance of a group.
struct Scope<'a> {
    nodes: &'a [Node],
    by_name: HashMap<&'a str, &'a Node>,
    /// `(to node, to input)` to `(from node, from output)` of the links Blender honours.
    links: HashMap<Source, Source>,
    /// The link targets, in document order.
    consumers: Vec<Source>,
    /// `"{group node}/"` for an instance, empty for the material.
    prefix: String,
    parent: Option<Parent<'a>>,
}

/// Where an inlined group instance sits: the scope of its group node, the node and the group it names.
#[derive(Clone, Copy)]
struct Parent<'a> {
    scope: usize,
    node: &'a Node,
    group: &'a Group,
}

struct Builder<'a> {
    doc: &'a Document,
    graph: Graph,
    warnings: Vec<String>,
    scopes: Vec<Scope<'a>>,
    /// `(scope, group node)` to the scope of its inlined instance.
    instances: HashMap<(usize, String), usize>,
    /// `(scope, node, socket)` to the plain output it finally comes from.
    memo: HashMap<(usize, String, String), Option<Source>>,
    active: HashSet<(usize, String, String)>,
}

impl<'a> Builder<'a> {
    /// A scope over `nodes` and `links`, dropping the links Blender ignores.
    fn scope(
        &mut self,
        nodes: &'a [Node],
        links: &[Link],
        prefix: String,
        parent: Option<Parent<'a>>,
    ) -> Result<usize, Error> {
        let mut by_name = HashMap::new();
        for node in nodes {
            if by_name.insert(node.name.as_str(), node).is_some() {
                return Err(bad(format!("two nodes named {prefix}{}", node.name)));
            }
        }
        let (mut map, mut consumers) = (HashMap::new(), Vec::new());
        for link in links.iter().filter(|l| !l.muted) {
            if !link.valid {
                let (from, to) = (&link.from, &link.to);
                self.warnings.push(format!(
                    "link {prefix}{}.{} → {prefix}{}.{}: invalid in Blender, dropped",
                    from.0, from.1, to.0, to.1
                ));
            } else if map.insert(link.to.clone(), link.from.clone()).is_none() {
                consumers.push(link.to.clone());
            }
        }
        self.scopes.push(Scope { nodes, by_name, links: map, consumers, prefix, parent });
        Ok(self.scopes.len() - 1)
    }

    fn full(&self, scope: usize, name: &str) -> String {
        format!("{}{name}", self.scopes[scope].prefix)
    }

    /// Adds the scope's plain nodes, inlines its groups and records where its plain nodes' inputs come from.
    fn populate(&mut self, id: usize) -> Result<(), Error> {
        for node in self.scopes[id].nodes {
            match node.kind.as_str() {
                GROUP if !node.mute => {
                    self.instance(id, node)?;
                }
                kind if is_structural(kind) => {}
                _ if node.mute => {}
                _ => self.add_node(id, node)?,
            }
        }
        for to in self.scopes[id].consumers.clone() {
            self.link_consumer(id, to)?;
        }
        Ok(())
    }

    fn add_node(&mut self, scope: usize, node: &Node) -> Result<(), Error> {
        let name = self.full(scope, &node.name);
        if self.graph.by_name.contains_key(&name) {
            return Err(bad(format!("two nodes named {name}")));
        }
        self.push(FlatNode {
            name,
            kind: node.kind.clone(),
            properties: node.properties.clone(),
            inputs: node.inputs.clone(),
            outputs: node.outputs.clone(),
        });
        Ok(())
    }

    fn push(&mut self, node: FlatNode) {
        self.graph.by_name.insert(node.name.clone(), self.graph.nodes.len());
        self.graph.nodes.push(node);
    }

    /// A value node named `name`, made once, and the output to read it from.
    fn value_node(&mut self, name: String, kind: &str, value: Option<Json>) -> Source {
        if !self.graph.by_name.contains_key(&name) {
            let output = Socket {
                identifier: VALUE_OUTPUT.to_string(),
                name: VALUE_OUTPUT.to_string(),
                kind: kind.to_string(),
                enabled: true,
                value,
            };
            self.push(FlatNode {
                name: name.clone(),
                kind: VALUE_KIND.to_string(),
                properties: serde_json::Map::new(),
                inputs: Vec::new(),
                outputs: vec![output],
            });
        }
        (name, VALUE_OUTPUT.to_string())
    }

    /// Records the final source of a link into a plain node; links into reroutes, groups and muted nodes are
    /// followed from their readers instead.
    fn link_consumer(&mut self, scope: usize, to: Source) -> Result<(), Error> {
        let Some(target) = self.scopes[scope].by_name.get(to.0.as_str()).copied() else { return Ok(()) };
        if is_structural(&target.kind) || target.mute {
            return Ok(());
        }
        if let Some(from) = self.resolve_input(scope, &to.0, &to.1)? {
            let key = (self.full(scope, &to.0), to.1);
            self.graph.links.insert(key, from);
        }
        Ok(())
    }

    /// The plain output feeding input `socket` of `node` in `scope`, `None` when it is unlinked.
    fn resolve_input(&mut self, scope: usize, node: &str, socket: &str) -> Result<Option<Source>, Error> {
        match self.scopes[scope].links.get(&(node.to_string(), socket.to_string())).cloned() {
            Some(from) => self.resolve(scope, from),
            None => Ok(None),
        }
    }

    /// The plain output (or value node) that `from` stands for once reroutes, muted nodes and groups are
    /// looked through.
    fn resolve(&mut self, scope: usize, from: Source) -> Result<Option<Source>, Error> {
        let key = (scope, from.0.clone(), from.1.clone());
        if let Some(done) = self.memo.get(&key) {
            return Ok(done.clone());
        }
        if !self.active.insert(key.clone()) {
            return Err(Error::Cycle { at: at("graph"), node: self.full(scope, &from.0) });
        }
        let found = self.follow(scope, &from.0, &from.1);
        self.active.remove(&key);
        let found = found?;
        self.memo.insert(key, found.clone());
        Ok(found)
    }

    fn follow(&mut self, scope: usize, name: &str, output: &str) -> Result<Option<Source>, Error> {
        let Some(node) = self.scopes[scope].by_name.get(name).copied() else { return Ok(None) };
        match node.kind.as_str() {
            REROUTE => self.resolve_input(scope, name, "Input"),
            FRAME | GROUP_OUTPUT => Ok(None),
            GROUP_INPUT => self.group_input(scope, output),
            _ if node.mute => self.bypass(scope, node, output),
            GROUP => self.group_output(scope, node, output),
            _ => Ok(Some((self.full(scope, name), output.to_string()))),
        }
    }

    /// What a muted node's `output` reads: the source of the input Blender passes through, or that input's value.
    fn bypass(&mut self, scope: usize, node: &Node, output: &str) -> Result<Option<Source>, Error> {
        let full = self.full(scope, &node.name);
        let Some((input, _)) = node.internal_links.iter().find(|(_, out)| out == output) else {
            self.warnings.push(format!(
                "{} \"{full}\": muted, output {output} has no pass-through; its readers keep their own values",
                node.kind
            ));
            return Ok(None);
        };
        if let Some(from) = self.resolve_input(scope, &node.name, input)? {
            return Ok(Some(from));
        }
        let Some(socket) = node.input(input) else { return Ok(None) };
        Ok(Some(self.value_node(format!("{full}.{input}"), &socket.kind, socket.value.clone())))
    }

    /// What the group input `socket` reads inside an instance: the group node's own input, or its value.
    fn group_input(&mut self, scope: usize, socket: &str) -> Result<Option<Source>, Error> {
        let Some(parent) = self.scopes[scope].parent else { return Ok(None) };
        if let Some(from) = self.resolve_input(parent.scope, &parent.node.name, socket)? {
            return Ok(Some(from));
        }
        let own = parent.node.input(socket);
        let item = parent.group.interface.iter().find(|i| i.in_out == "INPUT" && i.identifier == socket);
        let kind = match (own, item) {
            (Some(own), _) => own.kind.clone(),
            (None, Some(item)) => socket_kind(&item.socket_type),
            (None, None) => return Ok(None),
        };
        let value = own.and_then(|s| s.value.clone()).or_else(|| item.and_then(|i| i.value.clone()));
        let name = format!("{}.{socket}", self.full(parent.scope, &parent.node.name));
        Ok(Some(self.value_node(name, &kind, value)))
    }

    /// What the group node's `output` reads: the source of the same socket on the group's active Group Output.
    fn group_output(&mut self, scope: usize, node: &'a Node, output: &str) -> Result<Option<Source>, Error> {
        let inner = self.instance(scope, node)?;
        let nodes = self.scopes[inner].nodes;
        let outputs = || nodes.iter().filter(|n| n.kind == GROUP_OUTPUT);
        let Some(active) = outputs().find(|n| n.flag("is_active_output")).or_else(|| outputs().next()) else {
            return Ok(None);
        };
        self.resolve_input(inner, &active.name, output)
    }

    /// The scope of the group `node` names, inlined on first use.
    fn instance(&mut self, scope: usize, node: &'a Node) -> Result<usize, Error> {
        let key = (scope, node.name.clone());
        if let Some(&id) = self.instances.get(&key) {
            return Ok(id);
        }
        let full = self.full(scope, &node.name);
        let tree = node.text("node_tree").unwrap_or_default();
        let doc = self.doc;
        let Some(group) = doc.groups.iter().find(|g| g.name == tree) else {
            return Err(bad(format!("group node {full}: no group named {tree}")));
        };
        let mut up = Some(scope);
        while let Some(s) = up {
            let parent = self.scopes[s].parent;
            if parent.is_some_and(|p| p.group.name == group.name) {
                return Err(Error::Cycle { at: at("graph"), node: group.name.clone() });
            }
            up = parent.map(|p| p.scope);
        }
        let id = self.scope(&group.nodes, &group.links, format!("{full}/"), Some(Parent { scope, node, group }))?;
        self.instances.insert(key, id);
        self.populate(id)?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::parse;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../blender/fixtures/graphs");

    fn flat(name: &str) -> (Graph, Vec<String>) {
        let doc = parse(&std::fs::read_to_string(format!("{FIXTURES}/{name}.json")).unwrap()).unwrap();
        Graph::flatten(&doc, &doc.materials[0]).unwrap()
    }

    #[test]
    fn the_structure_fixture_inlines_its_group_follows_its_reroute_and_bypasses_its_muted_node() {
        let (g, warnings) = flat("structure");
        assert_eq!(warnings, Vec::<String>::new());
        let mut names: Vec<&str> = g.nodes.iter().map(|n| n.name.as_str()).collect();
        names.sort();
        assert_eq!(
            names,
            ["Gain.Socket_0", "Gain/Scale", "Material Output", "Principled BSDF", "Source", "Tint", "Unused Output"]
        );
        // Roughness ← Gain.Socket_1 ← Gain/Scale.Value; Gain/Scale.Value ← Group Input.Socket_0 ← value node (unlinked at Gain).
        assert_eq!(g.source("Principled BSDF", "Roughness"), Some(("Gain/Scale", "Value")));
        assert_eq!(g.source("Gain/Scale", "Value"), Some(("Gain.Socket_0", "Value")));
        let amount = g.node("Gain.Socket_0").unwrap();
        assert_eq!(amount.kind, VALUE_KIND);
        assert_eq!(amount.outputs[0].value, Some(serde_json::json!(0.5)));
        assert_eq!(amount.outputs[0].kind, "VALUE");
        // Metallic ← Reroute ← Muted Add.Value (internal link Value → Value) ← Source.Value.
        assert_eq!(g.source("Principled BSDF", "Metallic"), Some(("Source", "Value")));
        assert_eq!(g.source("Principled BSDF", "Base Color"), Some(("Tint", "Color")));
        assert_eq!(g.source("Principled BSDF", "IOR"), None);
        assert_eq!(g.output.as_deref(), Some("Material Output"));
    }

    #[test]
    fn a_muted_node_with_an_unlinked_input_hands_its_readers_that_value() {
        let text = r#"{"format": "shadewire-blender-graph", "version": 1, "producer": "", "groups": [], "images": [],
          "materials": [{"name": "m", "settings": {}, "output": "Out", "links": [
              {"from": ["Add", "Value"], "to": ["P", "Roughness"], "muted": false, "valid": true}],
            "nodes": [
              {"name": "Add", "type": "ShaderNodeMath", "mute": true, "properties": {"operation": "ADD"},
               "inputs": [{"identifier": "Value", "name": "Value", "type": "VALUE", "enabled": true, "value": 0.25},
                          {"identifier": "Value_001", "name": "Value", "type": "VALUE", "enabled": true, "value": 0.5}],
               "outputs": [{"identifier": "Value", "name": "Value", "type": "VALUE", "enabled": true}],
               "internal_links": [["Value", "Value"]]},
              {"name": "P", "type": "ShaderNodeBsdfPrincipled", "inputs": [
                 {"identifier": "Roughness", "name": "Roughness", "type": "VALUE", "enabled": true, "value": 0.5}],
               "outputs": [{"identifier": "BSDF", "name": "BSDF", "type": "SHADER", "enabled": true}]},
              {"name": "Out", "type": "ShaderNodeOutputMaterial", "inputs": [
                 {"identifier": "Surface", "name": "Surface", "type": "SHADER", "enabled": true}], "outputs": []}]}]}"#;
        let doc = parse(text).unwrap();
        let (g, warnings) = Graph::flatten(&doc, &doc.materials[0]).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(g.source("P", "Roughness"), Some(("Add.Value", "Value")));
        assert_eq!(g.node("Add.Value").unwrap().outputs[0].value, Some(serde_json::json!(0.25)));
        assert!(g.node("Add").is_none());
    }

    #[test]
    fn the_gltf_fixture_inlines_a_group_without_outputs_and_keeps_the_links_into_it() {
        let (g, warnings) = flat("gltf_output");
        assert!(warnings.is_empty(), "{warnings:?}");
        // The glTF group has no inner nodes: its inputs are linked from outside, and nothing reads its outputs.
        assert!(g.nodes.iter().all(|n| !n.name.starts_with("glTF Material Output/")));
        assert_eq!(g.source("Principled BSDF", "Specular IOR Level"), Some(("Specular", "Value")));
    }

    #[test]
    fn an_invalid_link_is_dropped_with_a_warning_and_a_missing_group_is_an_error() {
        let base = r#"{"format": "shadewire-blender-graph", "version": 1, "producer": "", "groups": [], "images": [],
          "materials": [{"name": "m", "settings": {}, "output": null, "links": [
              {"from": ["A", "Value"], "to": ["B", "Color"], "muted": false, "valid": false}],
            "nodes": [
              {"name": "A", "type": "ShaderNodeValue", "inputs": [], "outputs": [{"identifier": "Value", "name": "Value", "type": "VALUE", "enabled": true, "value": 1.0}]},
              {"name": "B", "type": "ShaderNodeInvert", "inputs": [{"identifier": "Color", "name": "Color", "type": "RGBA", "enabled": true, "value": [0,0,0,1]}], "outputs": []},
              {"name": "G", "type": "ShaderNodeGroup", "properties": {"node_tree": "Missing"}, "inputs": [], "outputs": []}]}]}"#;
        let doc = parse(base).unwrap();
        let err = Graph::flatten(&doc, &doc.materials[0]).unwrap_err();
        assert!(err.to_string().contains("no group named Missing"), "{err}");
        let without_group = base.replace(r#",
              {"name": "G", "type": "ShaderNodeGroup", "properties": {"node_tree": "Missing"}, "inputs": [], "outputs": []}"#, "");
        let doc = parse(&without_group).unwrap();
        let (g, warnings) = Graph::flatten(&doc, &doc.materials[0]).unwrap();
        assert_eq!(warnings, ["link A.Value → B.Color: invalid in Blender, dropped"]);
        assert_eq!(g.source("B", "Color"), None);
    }

    fn socket(id: &str) -> String {
        format!(r#"{{"identifier": "{id}", "name": "{id}", "type": "VALUE", "enabled": true, "value": 1.0}}"#)
    }

    fn node(name: &str, kind: &str, props: &str, inputs: &[&str], outputs: &[&str]) -> String {
        let list = |ids: &[&str]| ids.iter().map(|s| socket(s)).collect::<Vec<_>>().join(",");
        format!(
            r#"{{"name": "{name}", "type": "{kind}", "properties": {props}, "inputs": [{}], "outputs": [{}]}}"#,
            list(inputs),
            list(outputs)
        )
    }

    fn link(from: (&str, &str), to: (&str, &str)) -> String {
        format!(r#"{{"from": ["{}", "{}"], "to": ["{}", "{}"]}}"#, from.0, from.1, to.0, to.1)
    }

    fn document(groups: &[String], nodes: &[String], links: &[String]) -> Document {
        let text = format!(
            r#"{{"format": "shadewire-blender-graph", "version": 1, "groups": [{}], "materials":
              [{{"name": "m", "output": null, "nodes": [{}], "links": [{}]}}]}}"#,
            groups.join(","),
            nodes.join(","),
            links.join(",")
        );
        parse(&text).unwrap()
    }

    fn group(name: &str, nodes: &[String], links: &[String]) -> String {
        format!(r#"{{"name": "{name}", "nodes": [{}], "links": [{}]}}"#, nodes.join(","), links.join(","))
    }

    #[test]
    fn nested_groups_accumulate_their_prefix_and_pass_links_through_both_levels() {
        let go = |n: &str| node(n, GROUP_OUTPUT, "{}", &["Out"], &[]);
        let gi = |n: &str| node(n, GROUP_INPUT, "{}", &[], &["In"]);
        let inner = group(
            "Inner",
            &[gi("GI"), node("Mul", "ShaderNodeMath", "{}", &["Value"], &["Value"]), go("GO")],
            &[link(("GI", "In"), ("Mul", "Value")), link(("Mul", "Value"), ("GO", "Out"))],
        );
        let outer = group(
            "Outer",
            &[gi("GI"), node("In", GROUP, r#"{"node_tree": "Inner"}"#, &["In"], &["Out"]), go("GO")],
            &[link(("GI", "In"), ("In", "In")), link(("In", "Out"), ("GO", "Out"))],
        );
        let nodes = [
            node("Src", "ShaderNodeValue", "{}", &[], &["Value"]),
            node("Out", GROUP, r#"{"node_tree": "Outer"}"#, &["In"], &["Out"]),
            node("P", "ShaderNodeBsdfPrincipled", "{}", &["Roughness"], &[]),
        ];
        let links = [link(("Src", "Value"), ("Out", "In")), link(("Out", "Out"), ("P", "Roughness"))];
        let doc = document(&[inner, outer], &nodes, &links);
        let (g, warnings) = Graph::flatten(&doc, &doc.materials[0]).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(g.node("Out/In/Mul").is_some());
        assert_eq!(g.source("P", "Roughness"), Some(("Out/In/Mul", "Value")));
        assert_eq!(g.source("Out/In/Mul", "Value"), Some(("Src", "Value")));
    }

    #[test]
    fn a_group_inside_itself_and_a_loop_of_reroutes_are_cycles() {
        let recursive = group("T", &[node("Again", GROUP, r#"{"node_tree": "T"}"#, &[], &[])], &[]);
        let doc = document(&[recursive], &[node("G", GROUP, r#"{"node_tree": "T"}"#, &[], &[])], &[]);
        let err = Graph::flatten(&doc, &doc.materials[0]).unwrap_err();
        assert!(matches!(err, Error::Cycle { ref node, .. } if node == "T"), "{err}");
        let nodes = [
            node("R1", REROUTE, "{}", &["Input"], &["Output"]),
            node("R2", REROUTE, "{}", &["Input"], &["Output"]),
            node("B", "ShaderNodeInvert", "{}", &["Color"], &[]),
        ];
        let links = [
            link(("R1", "Output"), ("R2", "Input")),
            link(("R2", "Output"), ("R1", "Input")),
            link(("R1", "Output"), ("B", "Color")),
        ];
        let doc = document(&[], &nodes, &links);
        let err = Graph::flatten(&doc, &doc.materials[0]).unwrap_err();
        assert!(matches!(err, Error::Cycle { .. }), "{err}");
    }

    #[test]
    fn a_muted_output_without_a_pass_through_warns_and_two_nodes_of_one_name_are_an_error() {
        let mut muted = node("M", "ShaderNodeMath", "{}", &["Value"], &["Value", "Extra"]);
        muted =
            muted.replace(r#""properties""#, r#""mute": true, "internal_links": [["Value", "Value"]], "properties""#);
        let nodes = [muted, node("B", "ShaderNodeInvert", "{}", &["Color"], &[])];
        let doc = document(&[], &nodes, &[link(("M", "Extra"), ("B", "Color"))]);
        let (g, warnings) = Graph::flatten(&doc, &doc.materials[0]).unwrap();
        assert_eq!(
            warnings,
            ["ShaderNodeMath \"M\": muted, output Extra has no pass-through; its readers keep their own values"]
        );
        assert_eq!(g.source("B", "Color"), None);
        let twice = node("A", "ShaderNodeValue", "{}", &[], &["Value"]);
        let doc = document(&[], &[twice.clone(), twice], &[]);
        let err = Graph::flatten(&doc, &doc.materials[0]).unwrap_err();
        assert!(err.to_string().contains("two nodes named A"), "{err}");
    }

    #[test]
    fn every_fixture_flattens() {
        let mut count = 0;
        for entry in std::fs::read_dir(FIXTURES).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let doc = parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
            for material in &doc.materials {
                let (g, _) = Graph::flatten(&doc, material).unwrap_or_else(|e| panic!("{path:?}: {e}"));
                for node in &g.nodes {
                    let structural =
                        ["NodeReroute", "ShaderNodeGroup", "NodeGroupInput", "NodeGroupOutput", "NodeFrame"];
                    assert!(!structural.contains(&node.kind.as_str()), "{path:?}: {} survived", node.name);
                }
                count += 1;
            }
        }
        assert!(count >= 10, "{count}");
    }
}
