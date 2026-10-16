//! The `shadewire-blender-graph` document as Blender wrote it (`blender/shadewire/graph.py`): Blender's own
//! vocabulary, nothing interpreted.

use serde::Deserialize;
use serde_json::Value as Json;
use shadewire::{Error, Location};

pub const FORMAT: &str = "shadewire-blender-graph";
pub const VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize)]
pub struct Document {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub producer: String,
    pub materials: Vec<Material>,
    #[serde(default)]
    pub groups: Vec<Group>,
    #[serde(default)]
    pub images: Vec<Image>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Material {
    pub name: String,
    #[serde(default)]
    pub settings: serde_json::Map<String, Json>,
    /// The active Material Output's name; `None` when the tree has none.
    pub output: Option<String>,
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub links: Vec<Link>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Node {
    pub name: String,
    /// Blender's `bl_idname` (`ShaderNodeBsdfPrincipled`, `NodeReroute`, `ShaderNodeGroup`).
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub mute: bool,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub properties: serde_json::Map<String, Json>,
    #[serde(default)]
    pub inputs: Vec<Socket>,
    #[serde(default)]
    pub outputs: Vec<Socket>,
    /// `[input identifier, output identifier]` pairs Blender passes through when the node is muted.
    #[serde(default)]
    pub internal_links: Vec<(String, String)>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Socket {
    pub identifier: String,
    pub name: String,
    /// Blender's socket type: `VALUE`, `INT`, `BOOLEAN`, `VECTOR`, `ROTATION`, `RGBA`, `STRING`, `SHADER`, `CUSTOM`.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub value: Option<Json>,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Link {
    /// `(node name, output socket identifier)`.
    pub from: (String, String),
    /// `(node name, input socket identifier)`.
    pub to: (String, String),
    #[serde(default)]
    pub muted: bool,
    #[serde(default = "yes")]
    pub valid: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Group {
    pub name: String,
    #[serde(default)]
    pub interface: Vec<InterfaceItem>,
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub links: Vec<Link>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct InterfaceItem {
    pub identifier: String,
    pub name: String,
    /// `INPUT` or `OUTPUT`.
    pub in_out: String,
    pub socket_type: String,
    #[serde(default)]
    pub value: Option<Json>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Image {
    pub name: String,
    #[serde(default)]
    pub filepath: String,
    #[serde(default)]
    pub colorspace: String,
    #[serde(default)]
    pub alpha_mode: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub packed: bool,
}

impl Material {
    pub fn node(&self, name: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.name == name)
    }
}

impl Node {
    pub fn input(&self, identifier: &str) -> Option<&Socket> {
        self.inputs.iter().find(|s| s.identifier == identifier)
    }

    pub fn output(&self, identifier: &str) -> Option<&Socket> {
        self.outputs.iter().find(|s| s.identifier == identifier)
    }

    /// A string property (an enum identifier or a name), `None` when absent or not a string.
    pub fn text(&self, key: &str) -> Option<&str> {
        self.properties.get(key).and_then(Json::as_str)
    }

    pub fn flag(&self, key: &str) -> bool {
        self.properties.get(key).and_then(Json::as_bool).unwrap_or(false)
    }
}

pub(crate) fn at(message: impl Into<String>) -> Location {
    Location { file: None, line: 0, path: message.into(), source: None }
}

/// The document, checked to be a `shadewire-blender-graph` of a version this crate reads.
pub fn parse(text: &str) -> Result<Document, Error> {
    let doc: Document =
        serde_json::from_str(text).map_err(|e| Error::Parse { at: at("document"), message: e.to_string() })?;
    if doc.format != FORMAT {
        return Err(Error::Parse {
            at: at("document"),
            message: format!("format is {:?}, not {FORMAT:?}", doc.format),
        });
    }
    if doc.version != VERSION {
        return Err(Error::UnsupportedVersion { at: at("document"), version: doc.version.to_string() });
    }
    Ok(doc)
}

/// The material named `name`, or the document's only one.
pub fn pick_material<'a>(doc: &'a Document, name: Option<&str>) -> Result<&'a Material, Error> {
    match name {
        Some(name) => doc
            .materials
            .iter()
            .find(|m| m.name == name)
            .ok_or_else(|| Error::UnknownMaterial { name: name.to_string() }),
        None => match doc.materials.as_slice() {
            [] => Err(Error::NoMaterial),
            [one] => Ok(one),
            many => Err(Error::AmbiguousMaterial { names: many.iter().map(|m| m.name.clone()).collect() }),
        },
    }
}

/// The names of the document's materials, in document order.
pub fn material_names(text: &str) -> Result<Vec<String>, Error> {
    Ok(parse(text)?.materials.into_iter().map(|m| m.name).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../blender/fixtures/graphs");

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("{FIXTURES}/{name}.json")).unwrap()
    }

    #[test]
    fn every_fixture_parses_with_its_materials_groups_and_images() {
        let mut names = Vec::new();
        for entry in std::fs::read_dir(FIXTURES).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "json") {
                let doc = parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
                assert_eq!(doc.format, "shadewire-blender-graph");
                assert_eq!(doc.version, 1);
                names.push(path.file_stem().unwrap().to_string_lossy().to_string());
            }
        }
        names.sort();
        assert_eq!(names.len(), 10, "{names:?}");
        let structure = parse(&fixture("structure")).unwrap();
        assert_eq!(structure.groups.len(), 1);
        assert_eq!(structure.groups[0].interface[1].identifier, "Socket_0");
        assert_eq!(structure.groups[0].interface[1].value, Some(serde_json::json!(0.5)));
        let textured = parse(&fixture("textured")).unwrap();
        assert_eq!(textured.images[0].colorspace, "Non-Color");
        let principled = textured.materials[0].node("Principled BSDF").unwrap();
        assert_eq!(principled.kind, "ShaderNodeBsdfPrincipled");
        assert_eq!(principled.input("Roughness").unwrap().value, Some(serde_json::json!(0.5)));
        assert!(principled.input("No Such Socket").is_none());
        assert_eq!(textured.materials[0].links[0].from, ("Principled BSDF".to_string(), "BSDF".to_string()));
    }

    #[test]
    fn material_names_lists_every_material_in_order() {
        let text = r#"{"format": "shadewire-blender-graph", "version": 1, "producer": "test",
          "materials": [{"name": "b", "settings": {}, "output": null, "nodes": [], "links": []},
                        {"name": "a", "settings": {}, "output": null, "nodes": [], "links": []}],
          "groups": [], "images": []}"#;
        assert_eq!(material_names(text).unwrap(), ["b", "a"]);
    }

    #[test]
    fn the_wrong_format_version_or_malformed_json_is_a_parse_error() {
        let wrong = r#"{"format": "something-else", "version": 1, "producer": "", "materials": [], "groups": [], "images": []}"#;
        let err = parse(wrong).unwrap_err();
        assert!(matches!(err, Error::Parse { .. }), "{err}");
        assert!(err.to_string().contains("something-else"), "{err}");
        let newer = r#"{"format": "shadewire-blender-graph", "version": 2, "producer": "", "materials": [], "groups": [], "images": []}"#;
        assert!(matches!(parse(newer).unwrap_err(), Error::UnsupportedVersion { .. }));
        let err = parse("{not json").unwrap_err();
        assert!(matches!(err, Error::Parse { .. }), "{err}");
    }

    #[test]
    fn pick_material_follows_the_mtlx_rules() {
        let doc = parse(&fixture("lobes")).unwrap();
        assert_eq!(pick_material(&doc, None).unwrap().name, "lobes");
        assert_eq!(pick_material(&doc, Some("lobes")).unwrap().name, "lobes");
        assert!(matches!(pick_material(&doc, Some("nope")), Err(Error::UnknownMaterial { .. })));
        let two = parse(
            r#"{"format": "shadewire-blender-graph", "version": 1, "producer": "",
          "materials": [{"name": "a", "settings": {}, "output": null, "nodes": [], "links": []},
                        {"name": "b", "settings": {}, "output": null, "nodes": [], "links": []}],
          "groups": [], "images": []}"#,
        )
        .unwrap();
        assert!(matches!(pick_material(&two, None), Err(Error::AmbiguousMaterial { .. })));
        let none = parse(
            r#"{"format": "shadewire-blender-graph", "version": 1, "producer": "", "materials": [], "groups": [], "images": []}"#,
        )
        .unwrap();
        assert!(matches!(pick_material(&none, None), Err(Error::NoMaterial)));
    }
}
