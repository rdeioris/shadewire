//! Image Texture, Texture Coordinate, UV Map, Attribute, Color Attribute, Geometry, Normal Map, Bump, Mapping and
//! Tangent: the nodes that read the image files and the geometry, and the vector transforms between them.

use shadewire::{ContextRead, Error, NodeId, Space, Type, Value};
use shadewire_nodes::Arg;

use super::{Lowerer, missing_output};
use crate::document::Image;
use crate::graph::FlatNode;

const F: Type = Type::Float;
const V: Type = Type::Vector3;
const C: Type = Type::Color3;

impl<'a> Lowerer<'a> {
    /// The first UV set, a `vector2`.
    pub fn uv0(&mut self, origin: &str) -> NodeId {
        self.e.context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }, origin)
    }

    /// The `vector2` `uv` as the `vector3` Blender's coordinates are, with z zero.
    pub fn uv_as_vector3(&mut self, uv: NodeId, origin: &str) -> Result<NodeId, Error> {
        let (u, v, z) = (self.extract(uv, 0, origin)?, self.extract(uv, 1, origin)?, self.k(0.0, origin));
        self.combine3(u, v, z, V, origin)
    }

    /// The `vector3` `v` as a `vector2` (its z is dropped).
    pub fn vector3_as_uv(&mut self, v: NodeId, origin: &str) -> Result<NodeId, Error> {
        self.convert(v, Type::Vector2, origin)
    }

    /// The position in `space` (`"object"`, `"world"`, `"model"`).
    pub fn position(&mut self, space: &str, origin: &str) -> Result<NodeId, Error> {
        self.op("position", V, &[("space", Arg::Text(space.to_string()))], origin)
    }

    /// The shading normal in `space`.
    pub fn normal(&mut self, space: &str, origin: &str) -> Result<NodeId, Error> {
        self.op("normal", V, &[("space", Arg::Text(space.to_string()))], origin)
    }

    /// The document's image called `name`.
    pub fn image_of(&self, name: &str) -> Option<&Image> {
        self.images.iter().find(|image| image.name == name)
    }

    /// A zero vector, the value of an output this version cannot read.
    fn zero_vector(&mut self, origin: &str) -> NodeId {
        self.k3(0.0, V, origin)
    }
}

/// Image Texture: one `color4` sample per node, shared by `Color` and `Alpha`.
pub fn image(l: &mut Lowerer, node: &FlatNode, output: &str) -> Result<NodeId, Error> {
    let origin = format!("{}.{output}", node.name);
    if !matches!(output, "Color" | "Alpha") {
        return Err(missing_output(node, output));
    }
    let Some(sample) = image_sample(l, node)? else {
        return Ok(if output == "Color" { l.k3(0.0, C, &origin) } else { l.k(1.0, &origin) });
    };
    if output == "Color" { l.convert(sample, C, &origin) } else { l.extract(sample, 3, &origin) }
}

/// The node's `color4` sample, made once; `None` (with a warning) when the node has no image to read.
fn image_sample(l: &mut Lowerer, node: &FlatNode) -> Result<Option<NodeId>, Error> {
    let key = (node.name.clone(), "#sample".to_string());
    if let Some(&id) = l.memo.get(&key) {
        return Ok(Some(id));
    }
    let name = node.text("image").unwrap_or("");
    let Some(image) = l.image_of(name).cloned() else {
        l.warn(node, "has no image, black");
        return Ok(None);
    };
    let path = if image.filepath.is_empty() {
        l.warn(node, format!("image {} has no file path, its name is written", image.name));
        image.name.clone()
    } else {
        image.filepath.clone()
    };
    let colorspace = match image.colorspace.as_str() {
        "sRGB" => Some("srgb_texture"),
        "Non-Color" | "Linear Rec.709" => None,
        "ACEScg" => Some("acescg"),
        "Linear Rec.2020" => Some("lin_rec2020"),
        "Display P3" => Some("srgb_displayp3"),
        "Linear DCI-P3 D65" => Some("lin_displayp3"),
        other => {
            l.warn(node, format!("colorspace {other} unknown, samples read as they are"));
            None
        }
    };
    let projection = node.text("projection").unwrap_or("FLAT");
    if projection != "FLAT" {
        l.warn(node, format!("projection {projection} read as Flat"));
    }
    let address = match node.text("extension").unwrap_or("REPEAT") {
        "EXTEND" => "clamp",
        "CLIP" => "constant",
        "MIRROR" => "mirror",
        _ => "periodic",
    };
    let filter = match node.text("interpolation").unwrap_or("Linear") {
        "Closest" => "closest",
        "Cubic" | "Smart" => "cubic",
        _ => "linear",
    };
    let origin = node.name.as_str();
    let mut inputs = vec![
        ("file", Arg::File { path, colorspace: colorspace.map(str::to_string) }),
        ("uaddressmode", Arg::Text(address.to_string())),
        ("vaddressmode", Arg::Text(address.to_string())),
        ("filtertype", Arg::Text(filter.to_string())),
    ];
    if let Some(vector) = l.input_if_linked(node, "Vector", V)? {
        let uv = l.vector3_as_uv(vector, origin)?;
        inputs.push(("texcoord", Arg::Node(uv)));
    }
    let id = l.op("image", Type::Color4, &inputs, origin)?;
    l.memo.insert(key, id);
    Ok(Some(id))
}

/// Texture Coordinate: the coordinate systems the context can answer.
pub fn tex_coord(l: &mut Lowerer, node: &FlatNode, output: &str) -> Result<NodeId, Error> {
    let o = format!("{}.{output}", node.name);
    if node.properties.get("object").is_some_and(|object| !object.is_null()) {
        l.warn(node, "object ignored");
    }
    match output {
        "UV" => {
            let uv = l.uv0(&o);
            l.uv_as_vector3(uv, &o)
        }
        "Generated" => {
            l.warn(node, "Generated read as object position");
            l.position("object", &o)
        }
        "Object" => l.position("object", &o),
        "Normal" => l.normal("object", &o),
        "Camera" | "Window" | "Reflection" => {
            l.warn(node, format!("{output} not available, zero"));
            Ok(l.zero_vector(&o))
        }
        _ => Err(missing_output(node, output)),
    }
}

/// `uv_map` names a UV set; only the first is read.
fn warn_uv_map(l: &mut Lowerer, node: &FlatNode) {
    let map = node.text("uv_map").unwrap_or("");
    if !map.is_empty() {
        l.warn(node, format!("UV map \"{map}\" read as the first UV set"));
    }
}

/// UV Map: the first UV set as a vector.
pub fn uv_map(l: &mut Lowerer, node: &FlatNode, output: &str) -> Result<NodeId, Error> {
    if output != "UV" {
        return Err(missing_output(node, output));
    }
    warn_uv_map(l, node);
    let o = format!("{}.{output}", node.name);
    let uv = l.uv0(&o);
    l.uv_as_vector3(uv, &o)
}

/// Attribute: a geometry property by name; the other attribute types have no source here.
pub fn attribute(l: &mut Lowerer, node: &FlatNode, output: &str) -> Result<NodeId, Error> {
    let o = format!("{}.{output}", node.name);
    let ty = match output {
        "Color" => C,
        "Vector" => V,
        "Fac" => F,
        "Alpha" => {
            l.warn(node, "Alpha read as 1");
            return Ok(l.k(1.0, &o));
        }
        _ => return Err(missing_output(node, output)),
    };
    let kind = node.text("attribute_type").unwrap_or("GEOMETRY");
    let name = node.text("attribute_name").unwrap_or("");
    if kind != "GEOMETRY" || name.is_empty() {
        l.warn(node, format!("attribute {kind} \"{name}\" not available, zero"));
        return Ok(l.k_of(0.0, ty, &o));
    }
    l.op("geompropvalue", ty, &[("geomprop", Arg::Text(name.to_string()))], &o)
}

/// Color Attribute: the first vertex color set.
pub fn color_attribute(l: &mut Lowerer, node: &FlatNode, output: &str) -> Result<NodeId, Error> {
    let o = format!("{}.{output}", node.name);
    let layer = node.text("layer_name").unwrap_or("");
    if !layer.is_empty() {
        l.warn(node, format!("layer \"{layer}\" read as the first color set"));
    }
    let index = || ("index", Arg::Value(Value::Integer(0)));
    match output {
        "Color" => l.op("geomcolor", C, &[index()], &o),
        "Alpha" => {
            let color = l.op("geomcolor", Type::Color4, &[index()], &o)?;
            l.extract(color, 3, &o)
        }
        _ => Err(missing_output(node, output)),
    }
}

/// Geometry: position, normals, tangent and the view direction.
pub fn geometry(l: &mut Lowerer, node: &FlatNode, output: &str) -> Result<NodeId, Error> {
    let o = format!("{}.{output}", node.name);
    match output {
        "Position" => l.position("world", &o),
        "Normal" => l.normal("world", &o),
        "Tangent" => {
            let inputs = [("space", Arg::Text("world".to_string())), ("index", Arg::Value(Value::Integer(0)))];
            l.op("tangent", V, &inputs, &o)
        }
        "True Normal" => {
            l.warn(node, "True Normal read as the shading normal");
            l.normal("world", &o)
        }
        // Shadewire's view direction runs from the camera to the surface; Blender's Incoming, the other way.
        "Incoming" => {
            let view = l.e.context(ContextRead::ViewDirection(Space::World), &o);
            let minus = l.k(-1.0, &o);
            l.op2("multiply", V, view, minus, &o)
        }
        "Parametric" => {
            l.warn(node, "Parametric not available, zero");
            Ok(l.zero_vector(&o))
        }
        "Backfacing" | "Pointiness" | "Random Per Island" => {
            l.warn(node, format!("{output} not available, zero"));
            Ok(l.k(0.0, &o))
        }
        _ => Err(missing_output(node, output)),
    }
}

/// Normal Map: the color decoded as a normal, scaled by the strength.
pub fn normal_map(l: &mut Lowerer, node: &FlatNode, output: &str) -> Result<NodeId, Error> {
    if output != "Normal" {
        return Err(missing_output(node, output));
    }
    let o = format!("{}.{output}", node.name);
    let space = node.text("space").unwrap_or("TANGENT");
    if space != "TANGENT" {
        l.warn(node, format!("{space} space read as tangent space"));
    }
    warn_uv_map(l, node);
    let strength = l.input(node, "Strength", F)?;
    let color = l.input(node, "Color", V)?;
    l.op("normalmap", V, &[("in", Arg::Node(color)), ("scale", Arg::Node(strength))], &o)
}

/// Bump: the height field's slope along the screen perturbs the normal, by strength times distance. Written out
/// from the screen derivatives of the position and the height (Mikkelsen's bump mapping of unparametrized
/// surfaces), because MaterialX's `bump` needs a `heighttonormal` implementation no back end here has. A surface
/// with no area on screen (a zero determinant) keeps its normal.
pub fn bump(l: &mut Lowerer, node: &FlatNode, output: &str) -> Result<NodeId, Error> {
    if output != "Normal" {
        return Err(missing_output(node, output));
    }
    let o = format!("{}.{output}", node.name);
    let (strength, distance) = (l.input(node, "Strength", F)?, l.input(node, "Distance", F)?);
    let mut scale = l.op2("multiply", F, strength, distance, &o)?;
    if node.flag("invert") {
        let minus = l.k(-1.0, &o);
        scale = l.op2("multiply", F, scale, minus, &o)?;
    }
    let height = l.input(node, "Height", F)?;
    let height = l.op2("multiply", F, height, scale, &o)?;
    let normal = match l.input_if_linked(node, "Normal", V)? {
        Some(normal) => normal,
        None => l.normal("world", &o)?,
    };
    let position = l.position("world", &o)?;
    let (dpdx, dpdy) = (l.op1("shadewire_dpdx", V, position, &o)?, l.op1("shadewire_dpdy", V, position, &o)?);
    let (dhdx, dhdy) = (l.op1("shadewire_dpdx", F, height, &o)?, l.op1("shadewire_dpdy", F, height, &o)?);
    let r1 = l.op2("crossproduct", V, dpdy, normal, &o)?;
    let r2 = l.op2("crossproduct", V, normal, dpdx, &o)?;
    let det = l.op2("dotproduct", F, dpdx, r1, &o)?;
    let (along1, along2) = (l.op2("multiply", V, r1, dhdx, &o)?, l.op2("multiply", V, r2, dhdy, &o)?);
    let slope = l.op2("add", V, along1, along2, &o)?;
    let direction = l.op1("sign", F, det, &o)?;
    let gradient = l.op2("multiply", V, slope, direction, &o)?;
    let area = l.op1("absval", F, det, &o)?;
    let upright = l.op2("multiply", V, normal, area, &o)?;
    let tilted = l.op2("subtract", V, upright, gradient, &o)?;
    let bumped = l.op1("normalize", V, tilted, &o)?;
    let zero = l.k(0.0, &o);
    l.if_equal(det, zero, normal, bumped, V, &o)
}

const AXES: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// `v` turned by `degrees` about the axis `AXES[axis]`.
fn rotate(l: &mut Lowerer, v: NodeId, axis: usize, degrees: NodeId, o: &str) -> Result<NodeId, Error> {
    let axis = Arg::Value(Value::Vector3(AXES[axis]));
    l.op("rotate3d", V, &[("in", Arg::Node(v)), ("amount", Arg::Node(degrees)), ("axis", axis)], o)
}

/// Mapping: the vector scaled, turned by the XYZ Euler rotation and moved, or the same undone.
pub fn mapping(l: &mut Lowerer, node: &FlatNode, output: &str) -> Result<NodeId, Error> {
    if output != "Vector" {
        return Err(missing_output(node, output));
    }
    let o = format!("{}.{output}", node.name);
    let kind = node.text("vector_type").unwrap_or("POINT");
    let vector = l.input(node, "Vector", V)?;
    let location = l.input(node, "Location", V)?;
    let rotation = l.input(node, "Rotation", V)?;
    let scale = l.input(node, "Scale", V)?;
    // Blender's rotation is in radians, MaterialX's `rotate3d` in degrees.
    let to_degrees = l.k(180.0 / std::f32::consts::PI, &o);
    let minus = l.k(-1.0, &o);
    let mut degrees = [rotation; 3];
    let mut negated = [rotation; 3];
    for i in 0..3 {
        let radians = l.extract(rotation, i as u32, &o)?;
        degrees[i] = l.op2("multiply", F, radians, to_degrees, &o)?;
        negated[i] = l.op2("multiply", F, degrees[i], minus, &o)?;
    }
    // Blender turns counterclockwise about an axis and the `rotate3d` graph clockwise, so the angles handed to it are
    // negated: `negated` turns the way Blender's `degrees` do, and `degrees` the way that is undone.
    // Blender's XYZ Euler turns about X first, then Y, then Z.
    let forward = |l: &mut Lowerer, mut v: NodeId| -> Result<NodeId, Error> {
        for (axis, &angle) in negated.iter().enumerate() {
            v = rotate(l, v, axis, angle, &o)?;
        }
        Ok(v)
    };
    match kind {
        "TEXTURE" => {
            let mut v = l.op2("subtract", V, vector, location, &o)?;
            for (axis, &angle) in degrees.iter().enumerate().rev() {
                v = rotate(l, v, axis, angle, &o)?;
            }
            l.op2("divide", V, v, scale, &o)
        }
        "VECTOR" => {
            let scaled = l.op2("multiply", V, vector, scale, &o)?;
            forward(l, scaled)
        }
        "NORMAL" => {
            let scaled = l.op2("divide", V, vector, scale, &o)?;
            let turned = forward(l, scaled)?;
            l.op1("normalize", V, turned, &o)
        }
        _ => {
            let scaled = l.op2("multiply", V, vector, scale, &o)?;
            let turned = forward(l, scaled)?;
            l.op2("add", V, turned, location, &o)
        }
    }
}

/// Tangent: the UV tangent in world space.
pub fn tangent(l: &mut Lowerer, node: &FlatNode, output: &str) -> Result<NodeId, Error> {
    if output != "Tangent" {
        return Err(missing_output(node, output));
    }
    let o = format!("{}.{output}", node.name);
    if node.text("direction_type") == Some("RADIAL") {
        l.warn(node, "radial tangent read as the UV tangent");
    }
    warn_uv_map(l, node);
    let inputs = [("space", Arg::Text("world".to_string())), ("index", Arg::Value(Value::Integer(0)))];
    l.op("tangent", V, &inputs, &o)
}
