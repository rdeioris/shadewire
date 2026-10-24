//! The `shadewire-unreal-graph` document as the Unreal plugin wrote it (`unreal/Shadewire`): Unreal's own
//! vocabulary, nothing interpreted.

use serde::Deserialize;
use serde_json::Value as Json;
use shadewire::{Error, Location};

pub const FORMAT: &str = "shadewire-unreal-graph";
pub const VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize)]
pub struct Document {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub producer: String,
    #[serde(default)]
    pub substrate: bool,
    /// The materials the document was made for.
    pub materials: Vec<Material>,
    /// The parents of the instances among them, nearest first.
    #[serde(default)]
    pub parents: Vec<Material>,
    #[serde(default)]
    pub functions: Vec<Function>,
    #[serde(default)]
    pub textures: Vec<Texture>,
}

/// A `Material` or a material instance (`MaterialInstanceConstant`), by its `class`.
#[derive(Clone, Debug, Deserialize)]
pub struct Material {
    /// The object path, `/Game/Dir/M_Name.M_Name`.
    pub path: String,
    pub name: String,
    pub class: String,
    /// An instance's parent, by path; `None` for a material.
    pub parent: Option<String>,
    #[serde(default)]
    pub settings: serde_json::Map<String, Json>,
    #[serde(default)]
    pub pins: Vec<Pin>,
    #[serde(default)]
    pub expressions: Vec<Expression>,
    /// An instance's own overrides.
    #[serde(default)]
    pub overrides: Option<Overrides>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Overrides {
    #[serde(default)]
    pub scalar: serde_json::Map<String, Json>,
    #[serde(default)]
    pub vector: serde_json::Map<String, Json>,
    #[serde(default)]
    pub texture: serde_json::Map<String, Json>,
    #[serde(default)]
    pub static_switch: serde_json::Map<String, Json>,
    #[serde(default)]
    pub base_property_overrides: serde_json::Map<String, Json>,
}

/// A material input (`BaseColor`, `MaterialAttributes`) and what feeds it.
#[derive(Clone, Debug, Deserialize)]
pub struct Pin {
    /// The `EMaterialProperty` name without its `MP_` prefix.
    pub pin: String,
    pub from: Option<(String, usize)>,
    pub mask: Option<[u8; 4]>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Expression {
    /// The object name, unique within its material or function.
    pub name: String,
    /// The class name without its `U` prefix: `MaterialExpressionMultiply`.
    pub class: String,
    #[serde(default)]
    pub properties: serde_json::Map<String, Json>,
    #[serde(default)]
    pub inputs: Vec<Input>,
    #[serde(default)]
    pub outputs: Vec<Output>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Input {
    pub index: usize,
    pub name: String,
    /// `(expression, output index)`.
    pub from: Option<(String, usize)>,
    pub mask: Option<[u8; 4]>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Output {
    pub name: String,
    pub mask: Option<[u8; 4]>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Function {
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub expressions: Vec<Expression>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Texture {
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub class: String,
    /// The written file, relative to the document; empty when none was written.
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub srgb: bool,
    #[serde(default)]
    pub compression: String,
    #[serde(default)]
    pub filter: String,
    #[serde(default)]
    pub flip_green: bool,
    #[serde(default)]
    pub address_x: String,
    #[serde(default)]
    pub address_y: String,
}

impl Expression {
    /// The input called `name`, or at `index` when no input has the name (Unreal names some inputs `None`).
    pub fn input(&self, name: &str) -> Option<&Input> {
        self.inputs.iter().find(|i| i.name == name)
    }

    pub fn input_at(&self, index: usize) -> Option<&Input> {
        self.inputs.iter().find(|i| i.index == index)
    }

    pub fn number(&self, key: &str) -> Option<f32> {
        self.properties.get(key).and_then(Json::as_f64).map(|x| x as f32)
    }

    pub fn number_or(&self, key: &str, default: f32) -> f32 {
        self.number(key).unwrap_or(default)
    }

    /// A string property (an enum name, a name, a path), `None` when absent or not a string.
    pub fn text(&self, key: &str) -> Option<&str> {
        self.properties.get(key).and_then(Json::as_str)
    }

    pub fn flag(&self, key: &str) -> bool {
        self.properties.get(key).and_then(Json::as_bool).unwrap_or(false)
    }

    /// A vector property (`[r, g, b, a]`, `[x, y]`), as many components as it has.
    pub fn numbers(&self, key: &str) -> Option<Vec<f32>> {
        numbers(self.properties.get(key)?)
    }
}

impl Material {
    pub fn is_instance(&self) -> bool {
        self.parent.is_some() || self.overrides.is_some()
    }
}

/// A JSON array of numbers as `f32`s.
pub fn numbers(value: &Json) -> Option<Vec<f32>> {
    value.as_array()?.iter().map(|x| x.as_f64().map(|x| x as f32)).collect()
}

pub(crate) fn at(message: impl Into<String>) -> Location {
    Location { file: None, line: 0, path: message.into(), source: None }
}

/// The document, checked to be a `shadewire-unreal-graph` of a version this crate reads.
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

/// The material named `name` (by name or path), or the document's only one.
pub fn pick_material<'a>(doc: &'a Document, name: Option<&str>) -> Result<&'a Material, Error> {
    match name {
        Some(name) => doc
            .materials
            .iter()
            .find(|m| m.name == name || m.path == name)
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
