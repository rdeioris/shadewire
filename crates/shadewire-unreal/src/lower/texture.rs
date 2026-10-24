//! Texture samples, texture coordinates and the geometry reads, converted between Unreal's frame and Shadewire's:
//! Unreal is left-handed, Z up, in centimeters, with V running down its textures. Shadewire's world is the same
//! with Y negated, in meters; its V runs up.

use shadewire::{ContextRead, Error, NodeId, Space, Type, Value};
use shadewire_nodes::Arg;

use super::{Lowerer, short};
use crate::document::Expression;

/// Unreal's centimeters per Shadewire meter, with the Y flip: a point in Shadewire's world times this is Unreal's.
const TO_UNREAL_POINT: [f32; 3] = [100.0, -100.0, 100.0];
const TO_UNREAL_DIRECTION: [f32; 3] = [1.0, -1.0, 1.0];

impl<'a> Lowerer<'a> {
    /// `(u, 1 - v)`: Unreal's texture coordinate from Shadewire's, and back.
    pub fn flip_v(&mut self, uv: NodeId, origin: &str) -> Result<NodeId, Error> {
        let uv = self.cast(uv, 2, origin)?;
        let scale = self.kv(&[1.0, -1.0], origin);
        let offset = self.kv(&[0.0, 1.0], origin);
        let flipped = self.op2("multiply", uv, scale, origin)?;
        self.op2("add", flipped, offset, origin)
    }

    /// UV set `index` in Unreal's convention.
    pub fn unreal_uv(&mut self, index: u32, origin: &str) -> Result<NodeId, Error> {
        let uv = self.e.context(ContextRead::TexCoord { index, ty: Type::Vector2 }, origin);
        self.flip_v(uv, origin)
    }

    /// A Shadewire world point as Unreal's.
    pub fn unreal_point(&mut self, p: NodeId, origin: &str) -> Result<NodeId, Error> {
        let scale = self.e.constant(Value::Vector3(TO_UNREAL_POINT), origin);
        self.op2("multiply", p, scale, origin)
    }

    /// A Shadewire world direction as Unreal's; its own inverse.
    pub fn unreal_direction(&mut self, d: NodeId, origin: &str) -> Result<NodeId, Error> {
        let flip = self.e.constant(Value::Vector3(TO_UNREAL_DIRECTION), origin);
        self.op2("multiply", d, flip, origin)
    }

    /// The shading normal in Unreal's world.
    pub fn unreal_normal(&mut self, origin: &str) -> Result<NodeId, Error> {
        let n = self.e.context(ContextRead::Normal(Space::World), origin);
        self.unreal_direction(n, origin)
    }

    /// The direction from the surface to the camera in Unreal's world (Shadewire's view direction runs the other way).
    pub fn unreal_camera_vector(&mut self, origin: &str) -> Result<NodeId, Error> {
        let view = self.e.context(ContextRead::ViewDirection(Space::World), origin);
        let flip = self.e.constant(Value::Vector3([-1.0, 1.0, -1.0]), origin);
        self.op2("multiply", view, flip, origin)
    }
}

/// The texture an input of `x` names: what is linked to it (a texture object, a texture parameter or a function
/// input standing for one), else the expression's own `Texture`, with the instance's override of its parameter.
fn texture_path(l: &mut Lowerer, f: usize, x: &Expression) -> Option<String> {
    if let Some(from) = x.input("TextureObject").and_then(|i| i.from.clone()) {
        return linked_texture(l, f, &from.0, 0);
    }
    own_texture(l, x)
}

fn own_texture(l: &Lowerer, x: &Expression) -> Option<String> {
    if let Some(name) = x.text("ParameterName")
        && let Some(path) = l.params.texture.get(name)
    {
        return Some(path.clone());
    }
    x.text("Texture").map(str::to_string)
}

fn linked_texture(l: &mut Lowerer, f: usize, name: &str, depth: usize) -> Option<String> {
    let x = l.expression(f, name)?;
    if depth > 64 {
        return None;
    }
    match short(&x.class) {
        "TextureObject" | "TextureObjectParameter" => own_texture(l, x),
        "Reroute" | "NamedRerouteDeclaration" => {
            let from = x.input_at(0)?.from.clone()?;
            linked_texture(l, f, &from.0, depth + 1)
        }
        "NamedRerouteUsage" => {
            let declaration = x.text("Declaration")?.to_string();
            linked_texture(l, f, &declaration, depth + 1)
        }
        "FunctionInput" => {
            let (caller, call) = l.caller(f)?;
            let position = super::params::function_input_position(l, f, x)?;
            match call.input_at(position).and_then(|i| i.from.clone()) {
                Some(from) => linked_texture(l, caller, &from.0, depth + 1),
                None => {
                    let from = x.input("Preview")?.from.clone()?;
                    linked_texture(l, f, &from.0, depth + 1)
                }
            }
        }
        _ => None,
    }
}

/// TextureSample and TextureSampleParameter2D: one `RGBA` sample, which the outputs' masks pick from.
pub fn sample(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let key = (f, x.name.clone(), usize::MAX);
    if let Some(&id) = l.memo.get(&key) {
        return Ok(id);
    }
    let o = l.origin(f, x);
    let Some(path) = texture_path(l, f, x) else {
        l.warn(f, x, "no texture, read as 0");
        return Ok(l.zero(4, &o));
    };
    let texture = l.texture(&path).cloned();
    let file = match &texture {
        Some(t) if !t.file.is_empty() => t.file.clone(),
        _ => {
            l.warn(f, x, format!("texture {path} has no file, its asset path is written"));
            path.clone()
        }
    };
    let sampler = x.text("SamplerType").unwrap_or("SAMPLERTYPE_Color").to_string();
    let srgb = texture.as_ref().is_some_and(|t| t.srgb);
    let colorspace = (srgb && matches!(sampler.as_str(), "SAMPLERTYPE_Color" | "SAMPLERTYPE_External"))
        .then(|| "srgb_texture".to_string());
    let address = |mode: Option<&str>| match mode {
        Some("TA_Clamp") => "clamp",
        Some("TA_Mirror") => "mirror",
        _ => "periodic",
    };
    let (u_mode, v_mode) = match x.text("SamplerSource").unwrap_or("SSM_FromTextureAsset") {
        "SSM_Clamp_WorldGroupSettings" => ("clamp", "clamp"),
        "SSM_Wrap_WorldGroupSettings" | "SSM_TerrainWeightmapGroup" => ("periodic", "periodic"),
        _ => (
            address(texture.as_ref().map(|t| t.address_x.as_str())),
            address(texture.as_ref().map(|t| t.address_y.as_str())),
        ),
    };
    let filter = match texture.as_ref().map(|t| t.filter.as_str()) {
        Some("TF_Nearest") => "closest",
        _ => "linear",
    };
    if x.text("MipValueMode").is_some_and(|m| m != "TMVM_None") {
        l.warn(f, x, "mip value mode ignored");
    }
    // Linked coordinates are Unreal's and flip back to Shadewire's; a UV set is read as it is.
    let uv = match l.input(f, x, "Coordinates")? {
        Some(uv) => l.flip_v(uv, &o)?,
        None => {
            let index = x.number_or("ConstCoordinate", 0.0) as u32;
            l.e.context(ContextRead::TexCoord { index, ty: Type::Vector2 }, &o)
        }
    };
    let inputs = vec![
        ("file", Arg::File { path: file, colorspace }),
        ("uaddressmode", Arg::Text(u_mode.to_string())),
        ("vaddressmode", Arg::Text(v_mode.to_string())),
        ("filtertype", Arg::Text(filter.to_string())),
        ("texcoord", Arg::Node(uv)),
    ];
    let rgba = l.op("image", Type::Color4, &inputs, &o)?;
    let rgba = l.cast(rgba, 4, &o)?;
    let id = if sampler == "SAMPLERTYPE_Normal" {
        let flip_green = texture.as_ref().is_some_and(|t| t.flip_green);
        let two_channel = texture.as_ref().is_some_and(|t| t.compression == "TC_Normalmap");
        decode_normal(l, rgba, flip_green, two_channel, &o)?
    } else {
        rgba
    };
    l.memo.insert(key, id);
    Ok(id)
}

/// A normal map sample in `[0, 1]` decoded to `[-1, 1]`: green negated when the asset flips it at import, Z rebuilt
/// from X and Y when the asset stores two channels, as Unreal's BC5 normal maps do.
fn decode_normal(l: &mut Lowerer, rgba: NodeId, flip_green: bool, two_channel: bool, o: &str) -> Result<NodeId, Error> {
    let rgb = l.cast(rgba, 3, o)?;
    let scale = l.kv(&[2.0, if flip_green { -2.0 } else { 2.0 }, 2.0], o);
    let offset = l.kv(&[-1.0, if flip_green { 1.0 } else { -1.0 }, -1.0], o);
    let scaled = l.op2("multiply", rgb, scale, o)?;
    let mut xyz = l.op2("add", scaled, offset, o)?;
    if two_channel {
        let xy = l.cast(xyz, 2, o)?;
        let length2 = l.op("dotproduct", Type::Float, &[("in1", Arg::Node(xy)), ("in2", Arg::Node(xy))], o)?;
        let one = l.k(1.0, o);
        let rest = l.op2("subtract", one, length2, o)?;
        let rest = super::math::clamp01(l, rest, o)?;
        let z = l.op1("sqrt", rest, o)?;
        let (x, y) = (l.channel(xy, 0, o)?, l.channel(xy, 1, o)?);
        xyz = l.combine(&[x, y, z], o)?;
    }
    let one = l.k(1.0, o);
    let (x, y, z) = (l.channel(xyz, 0, o)?, l.channel(xyz, 1, o)?, l.channel(xyz, 2, o)?);
    l.combine(&[x, y, z, one], o)
}

/// UV set `CoordinateIndex`, scaled by `UTiling` and `VTiling`.
pub fn texture_coordinate(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    if x.flag("UnMirrorU") || x.flag("UnMirrorV") {
        l.warn(f, x, "un-mirroring ignored");
    }
    let uv = l.unreal_uv(x.number_or("CoordinateIndex", 0.0) as u32, &o)?;
    let (u, v) = (x.number_or("UTiling", 1.0), x.number_or("VTiling", 1.0));
    if u == 1.0 && v == 1.0 {
        return Ok(uv);
    }
    let tiling = l.kv(&[u, v], &o);
    l.op2("multiply", uv, tiling, &o)
}

/// `Coordinate + Time * Speed`, the offset's fractional part when `bFractionalPart` is set.
pub fn panner(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let coordinate = match l.input(f, x, "Coordinate")? {
        Some(id) => l.cast(id, 2, &o)?,
        None => l.unreal_uv(x.number_or("ConstCoordinate", 0.0) as u32, &o)?,
    };
    let time = match l.input(f, x, "Time")? {
        Some(id) => l.cast(id, 1, &o)?,
        None => l.e.context(ContextRead::Time, &o),
    };
    let speed = match l.input(f, x, "Speed")? {
        Some(id) => l.cast(id, 2, &o)?,
        None => l.kv(&[x.number_or("SpeedX", 0.0), x.number_or("SpeedY", 0.0)], &o),
    };
    let mut offset = l.op2("multiply", speed, time, &o)?;
    if x.flag("bFractionalPart") {
        offset = l.op1("fract", offset, &o)?;
    }
    l.op2("add", coordinate, offset, &o)
}

/// The scene time, wrapped by `Period` when `bOverride_Period` is set.
pub fn time(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let t = l.e.context(ContextRead::Time, &o);
    let period = x.number_or("Period", 0.0);
    if x.flag("bOverride_Period") && period > 0.0 {
        let p = l.k(period, &o);
        return l.op2("modulo", t, p, &o);
    }
    Ok(t)
}

pub fn world_position(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    if x.text("WorldPositionShaderOffset").is_some_and(|m| m != "WPT_Default") {
        l.warn(f, x, "read without the shader offset choice, as the absolute position");
    }
    let p = l.e.context(ContextRead::Position(Space::World), &o);
    l.unreal_point(p, &o)
}

pub fn vertex_normal(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    l.unreal_normal(&o)
}

/// The pixel normal is the normal after the Normal pin; it reads as the geometry's normal.
pub fn pixel_normal(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    l.warn(f, x, "read as the vertex normal");
    l.unreal_normal(&o)
}

pub fn camera_vector(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    l.unreal_camera_vector(&o)
}

/// The first vertex color set, `RGBA`.
pub fn vertex_color(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let index = ("index", Arg::Value(Value::Integer(0)));
    let color = l.op("geomcolor", Type::Color4, &[index], &o)?;
    l.cast(color, 4, &o)
}

/// Unreal's Fresnel: `base + (1 - base) * |1 - max(0, N·V)| ^ exponent`.
pub fn fresnel(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let normal = match l.input(f, x, "Normal")? {
        Some(id) => l.cast(id, 3, &o)?,
        None => {
            l.warn(f, x, "the pixel normal is read as the vertex normal");
            l.unreal_normal(&o)?
        }
    };
    let view = l.unreal_camera_vector(&o)?;
    let exponent = l.input_or(f, x, "ExponentIn", "Exponent", 5.0)?;
    let base = l.input_or(f, x, "BaseReflectFractionIn", "BaseReflectFraction", 0.04)?;
    let dot = l.op("dotproduct", Type::Float, &[("in1", Arg::Node(normal)), ("in2", Arg::Node(view))], &o)?;
    let (zero, one) = (l.k(0.0, &o), l.k(1.0, &o));
    let facing = l.op2("max", zero, dot, &o)?;
    let rim = l.op2("subtract", one, facing, &o)?;
    let rim = l.op1("absval", rim, &o)?;
    let rim = l.op2("power", rim, exponent, &o)?;
    let rest = l.op2("subtract", one, base, &o)?;
    let scaled = l.op2("multiply", rim, rest, &o)?;
    l.op2("add", scaled, base, &o)
}
