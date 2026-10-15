//! Noise, Voronoi, White Noise, Checker, Gradient and Light Path: the procedural patterns, written over MaterialX's
//! noise nodes and `checkerboard`, and the constants Light Path stands for.

use shadewire::{Error, NodeId, Type, Value};
use shadewire_nodes::Arg;

use super::values::socket_value;
use super::{Lowerer, missing_output};
use crate::graph::FlatNode;

const F: Type = Type::Float;
const V: Type = Type::Vector3;
const C: Type = Type::Color3;

/// The most octaves a Noise Texture's Detail is read as.
const MAX_OCTAVES: i32 = 16;

/// The point a procedural node is evaluated at: a position, or a UV for the 2D nodedefs.
struct Point {
    two_d: bool,
    at: NodeId,
}

impl Point {
    /// The nodedef input name the point goes to.
    fn input(&self) -> &'static str {
        if self.two_d { "texcoord" } else { "position" }
    }

    /// The `2d` or `3d` the noise nodes' categories end in.
    fn dimensions(&self) -> &'static str {
        if self.two_d { "2d" } else { "3d" }
    }
}

/// The coordinates a texture reads: its `Vector` when linked, else the object position (Blender's implicit
/// Generated coordinates), with a warning.
fn coordinates(l: &mut Lowerer, node: &FlatNode, origin: &str) -> Result<NodeId, Error> {
    match l.input_if_linked(node, "Vector", V)? {
        Some(vector) => Ok(vector),
        None => {
            l.warn(node, "unlinked Vector read as object position");
            l.position("object", origin)
        }
    }
}

/// Whether `socket` is on the node, or linked.
fn has(l: &Lowerer, node: &FlatNode, socket: &str) -> bool {
    node.input(socket).is_some() || l.linked(node, socket)
}

/// The point a noise-like node reads, by its `dimensions_key` property: the coordinates times `scale` (a socket
/// name) as a position, as a UV for `2D`; `4D` drops W, and `1D` reads (W times scale, 0, 0).
fn point(l: &mut Lowerer, node: &FlatNode, dimensions_key: &str, scale: Option<&str>, o: &str) -> Result<Point, Error> {
    let dimensions = node.text(dimensions_key).unwrap_or("3D");
    let scale = scale.filter(|s| has(l, node, s));
    if dimensions == "1D" {
        l.warn(node, "1D noise read as 3D over (W, 0, 0)");
        let mut w = l.input(node, "W", F)?;
        if let Some(scale) = scale {
            let scale = l.input(node, scale, F)?;
            w = l.op2("multiply", F, w, scale, o)?;
        }
        let zero = l.k(0.0, o);
        let at = l.combine3(w, zero, zero, V, o)?;
        return Ok(Point { two_d: false, at });
    }
    if dimensions == "4D" {
        l.warn(node, "4D noise read as 3D (W dropped)");
    }
    let mut at = coordinates(l, node, o)?;
    if let Some(scale) = scale {
        let scale = l.input(node, scale, F)?;
        at = l.op2("multiply", V, at, scale, o)?;
    }
    if dimensions == "2D" {
        let uv = l.vector3_as_uv(at, o)?;
        return Ok(Point { two_d: true, at: uv });
    }
    Ok(Point { two_d: false, at })
}

/// A cell noise of `point`, as a gray color when `ty` is a color.
fn cell_noise(l: &mut Lowerer, point: &Point, ty: Type, o: &str) -> Result<NodeId, Error> {
    let category = format!("cellnoise{}", point.dimensions());
    let noise = l.op(&category, F, &[(point.input(), Arg::Node(point.at))], o)?;
    l.convert(noise, ty, o)
}

/// The octaves Detail stands for: Blender's detail is the count minus one, and its fractional part blends the last
/// octave in, which is dropped. A linked Detail has no constant to read; a missing one leaves the nodedef's.
fn octaves(l: &mut Lowerer, node: &FlatNode) -> Option<i32> {
    if l.linked(node, "Detail") {
        l.warn(node, "linked Detail read as 3 octaves");
        return Some(3);
    }
    match node.input("Detail").and_then(socket_value) {
        Some(Value::Float(detail)) => Some((detail.floor() as i32 + 1).clamp(1, MAX_OCTAVES)),
        _ => None,
    }
}

/// `mx_fractal2d`, which the IR has no node for: the octaves of `noise2d` summed, each octave's point `lacunarity`
/// times the last and its weight `diminish` times the last (the nodedef's 2 and 0.5 when the sockets are missing).
fn fractal_2d(
    l: &mut Lowerer,
    uv: NodeId,
    ty: Type,
    octaves: i32,
    lacunarity: Option<NodeId>,
    diminish: Option<NodeId>,
    o: &str,
) -> Result<NodeId, Error> {
    let lacunarity = lacunarity.unwrap_or_else(|| l.k(2.0, o));
    let diminish = diminish.unwrap_or_else(|| l.k(0.5, o));
    let (mut point, mut weight) = (uv, l.k(1.0, o));
    let mut sum: Option<NodeId> = None;
    for octave in 0..octaves {
        if octave > 0 {
            point = l.op2("multiply", Type::Vector2, point, lacunarity, o)?;
            weight = l.op2("multiply", F, weight, diminish, o)?;
        }
        let noise = l.op("noise2d", ty, &[("texcoord", Arg::Node(point))], o)?;
        let term = l.op2("multiply", ty, noise, weight, o)?;
        sum = Some(match sum {
            Some(sum) => l.op2("add", ty, sum, term, o)?,
            None => term,
        });
    }
    Ok(sum.expect("at least one octave"))
}

/// Noise Texture: fractional Brownian motion over the scaled coordinates.
pub fn noise(l: &mut Lowerer, node: &FlatNode, output: &str) -> Result<NodeId, Error> {
    let ty = match output {
        "Fac" => F,
        "Color" => C,
        _ => return Err(missing_output(node, output)),
    };
    let o = format!("{}.{output}", node.name);
    let kind = node.text("noise_type").unwrap_or("FBM");
    if kind != "FBM" {
        l.warn(node, format!("{kind} read as fBM"));
    }
    let distorted = matches!(node.input("Distortion").and_then(socket_value), Some(Value::Float(d)) if d != 0.0);
    if l.linked(node, "Distortion") || distorted {
        l.warn(node, "Distortion dropped");
    }
    let p = point(l, node, "noise_dimensions", Some("Scale"), &o)?;
    let octaves = octaves(l, node);
    let diminish = if has(l, node, "Roughness") { Some(l.input(node, "Roughness", F)?) } else { None };
    let lacunarity = if has(l, node, "Lacunarity") { Some(l.input(node, "Lacunarity", F)?) } else { None };
    let fractal = if p.two_d {
        fractal_2d(l, p.at, ty, octaves.unwrap_or(3), lacunarity, diminish, &o)?
    } else {
        let mut inputs = vec![("position", Arg::Node(p.at))];
        if let Some(octaves) = octaves {
            inputs.push(("octaves", Arg::Value(Value::Integer(octaves))));
        }
        if let Some(diminish) = diminish {
            inputs.push(("diminish", Arg::Node(diminish)));
        }
        if let Some(lacunarity) = lacunarity {
            inputs.push(("lacunarity", Arg::Node(lacunarity)));
        }
        l.op("fractal3d", ty, &inputs, &o)?
    };
    if !node.flag("normalize") {
        return Ok(fractal);
    }
    let half = l.k_of(0.5, ty, &o);
    let halved = l.op2("multiply", ty, fractal, half, &o)?;
    l.op2("add", ty, halved, half, &o)
}

/// Voronoi Texture: the nearest or second-nearest feature point's distance from `worleynoise`; the other features
/// read as the nearest.
pub fn voronoi(l: &mut Lowerer, node: &FlatNode, output: &str) -> Result<NodeId, Error> {
    let o = format!("{}.{output}", node.name);
    match output {
        "W" => {
            l.warn(node, "W read as 0");
            return Ok(l.k(0.0, &o));
        }
        "Radius" => {
            l.warn(node, "Radius read as the F1 distance");
            return l.output(&node.name, "Distance");
        }
        "Distance" | "Color" | "Position" => {}
        _ => return Err(missing_output(node, output)),
    }
    let p = point(l, node, "voronoi_dimensions", Some("Scale"), &o)?;
    match output {
        "Color" => {
            l.warn(node, "Color read as a gray cell noise");
            cell_noise(l, &p, C, &o)
        }
        "Position" => {
            l.warn(node, "Position read as the input position");
            if p.two_d { l.uv_as_vector3(p.at, &o) } else { Ok(p.at) }
        }
        _ => {
            let metric = node.text("distance").unwrap_or("EUCLIDEAN");
            if metric != "EUCLIDEAN" {
                l.warn(node, format!("{metric} read as Euclidean"));
            }
            if node.flag("normalize") {
                l.warn(node, "Normalize dropped");
            }
            let feature = node.text("feature").unwrap_or("F1");
            if !matches!(feature, "F1" | "F2") {
                l.warn(node, format!("feature {feature} read as F1"));
            }
            let mut inputs = vec![(p.input(), Arg::Node(p.at))];
            if has(l, node, "Randomness") {
                let jitter = l.input(node, "Randomness", F)?;
                inputs.push(("jitter", Arg::Node(jitter)));
            }
            let category = format!("worleynoise{}", p.dimensions());
            if feature == "F2" {
                // The vector output is (F1, F2, F3), nearest first.
                let nearest = l.op(&category, V, &inputs, &o)?;
                l.extract(nearest, 1, &o)
            } else {
                l.op(&category, F, &inputs, &o)
            }
        }
    }
}

/// White Noise Texture: a cell noise of the coordinates, one value per unit cell.
pub fn white_noise(l: &mut Lowerer, node: &FlatNode, output: &str) -> Result<NodeId, Error> {
    let ty = match output {
        "Value" => F,
        "Color" => C,
        _ => return Err(missing_output(node, output)),
    };
    let o = format!("{}.{output}", node.name);
    let p = point(l, node, "noise_dimensions", None, &o)?;
    if ty == C {
        l.warn(node, "Color read as a gray cell noise");
    }
    cell_noise(l, &p, ty, &o)
}

/// Checker Texture: `checkerboard` over the UV, tiled `Scale` times in both directions.
pub fn checker(l: &mut Lowerer, node: &FlatNode, output: &str) -> Result<NodeId, Error> {
    if !matches!(output, "Color" | "Fac") {
        return Err(missing_output(node, output));
    }
    let o = format!("{}.{output}", node.name);
    let uv = match l.input_if_linked(node, "Vector", V)? {
        Some(vector) => {
            l.warn(node, "checked over x and y only");
            l.vector3_as_uv(vector, &o)?
        }
        None => {
            l.warn(node, "unlinked Vector read as UV");
            l.uv0(&o)
        }
    };
    let mut inputs = vec![("texcoord", Arg::Node(uv))];
    if has(l, node, "Scale") {
        let scale = l.input(node, "Scale", F)?;
        let tiling = l.op("combine2", Type::Vector2, &[("in1", Arg::Node(scale)), ("in2", Arg::Node(scale))], &o)?;
        inputs.push(("uvtiling", Arg::Node(tiling)));
    }
    if output == "Color" {
        for (name, socket) in [("color1", "Color1"), ("color2", "Color2")] {
            if has(l, node, socket) {
                let color = l.input(node, socket, C)?;
                inputs.push((name, Arg::Node(color)));
            }
        }
        return l.op("checkerboard", C, &inputs, &o);
    }
    let (white, black) = (l.k3(1.0, C, &o), l.k3(0.0, C, &o));
    inputs.push(("color1", Arg::Node(white)));
    inputs.push(("color2", Arg::Node(black)));
    let pattern = l.op("checkerboard", C, &inputs, &o)?;
    l.extract(pattern, 0, &o)
}

/// Gradient Texture: Blender's gradient functions over the coordinates, written out.
pub fn gradient(l: &mut Lowerer, node: &FlatNode, output: &str) -> Result<NodeId, Error> {
    let o = format!("{}.{output}", node.name);
    match output {
        "Color" => {
            let fac = l.output(&node.name, "Fac")?;
            return l.convert(fac, C, &o);
        }
        "Fac" => {}
        _ => return Err(missing_output(node, output)),
    }
    let v = coordinates(l, node, &o)?;
    let (x, y) = (l.extract(v, 0, &o)?, l.extract(v, 1, &o)?);
    let zero = l.k(0.0, &o);
    let kind = node.text("gradient_type").unwrap_or("LINEAR");
    match kind {
        "QUADRATIC" => {
            let x = l.op2("max", F, x, zero, &o)?;
            l.op2("multiply", F, x, x, &o)
        }
        "EASING" => {
            let t = l.clamp01(x, F, &o)?;
            let squared = l.op2("multiply", F, t, t, &o)?;
            let cubed = l.op2("multiply", F, squared, t, &o)?;
            let (three, two) = (l.k(3.0, &o), l.k(2.0, &o));
            let rising = l.op2("multiply", F, three, squared, &o)?;
            let falling = l.op2("multiply", F, two, cubed, &o)?;
            l.op2("subtract", F, rising, falling, &o)
        }
        "DIAGONAL" => {
            let sum = l.op2("add", F, x, y, &o)?;
            let half = l.k(0.5, &o);
            l.op2("multiply", F, sum, half, &o)
        }
        "SPHERICAL" | "QUADRATIC_SPHERE" => {
            let length = l.op1("magnitude", F, v, &o)?;
            let one = l.k(1.0, &o);
            let inside = l.op2("subtract", F, one, length, &o)?;
            let s = l.op2("max", F, inside, zero, &o)?;
            if kind == "SPHERICAL" { Ok(s) } else { l.op2("multiply", F, s, s, &o) }
        }
        "RADIAL" => {
            let angle = l.op("atan2", F, &[("iny", Arg::Node(y)), ("inx", Arg::Node(x))], &o)?;
            let turn = l.k(std::f32::consts::TAU, &o);
            let turns = l.op2("divide", F, angle, turn, &o)?;
            let half = l.k(0.5, &o);
            l.op2("add", F, turns, half, &o)
        }
        _ => {
            if kind != "LINEAR" {
                l.warn(node, format!("gradient {kind} read as Linear"));
            }
            Ok(x)
        }
    }
}

/// Light Path: the camera ray is the only ray this version shades for, so Is Camera Ray is 1 and the rest 0.
pub fn light_path(l: &mut Lowerer, node: &FlatNode, output: &str) -> Result<NodeId, Error> {
    if node.output(output).is_none() {
        return Err(missing_output(node, output));
    }
    l.warn(node, "constants: Is Camera Ray 1, the rest 0");
    let o = format!("{}.{output}", node.name);
    Ok(l.k(if output == "Is Camera Ray" { 1.0 } else { 0.0 }, &o))
}
