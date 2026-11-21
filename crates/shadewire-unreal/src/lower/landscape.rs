//! The landscape expressions: a layer's paint weight as a geometric property the renderer supplies, and the nodes
//! built on it (LandscapeLayerSample, LandscapeLayerWeight, LandscapeLayerSwitch, LandscapeVisibilityMask,
//! LandscapeLayerBlend), plus LandscapeLayerCoords and LandscapeGrassOutput.
//!
//! Unreal reads a layer's weight from the component's weightmaps and compiles a shader per combination of the layers a
//! component paints, leaving out the ones it does not (`StaticTerrainLayerWeight` returns nothing). Here every
//! layer's weight is the geomprop `landscape_layer_<name>` (the name with anything but letters, digits and `_` made
//! `_`), defaulting to 0: a layer a component does not paint reads 0, which gives what Unreal's permutation gives
//! (a height-blended layer is the exception by at most its 0.0001 floor).

use shadewire::{ContextRead, Error, NodeId, Type, Value};
use shadewire_nodes::Arg;

use super::Lowerer;
use crate::document::Expression;

/// The geomprop a landscape layer's weight reads.
pub fn layer_geomprop(layer: &str) -> String {
    let name: String = layer.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    format!("landscape_layer_{name}")
}

/// The paint weight of `layer`, 0 where it is not painted.
pub fn layer_weight(l: &mut Lowerer, layer: &str, origin: &str) -> NodeId {
    let name = layer_geomprop(layer);
    l.e.geomprop_defaults.entry(name.clone()).or_insert(Value::Float(0.0));
    l.e.context(ContextRead::GeomProp { name, ty: Type::Float }, origin)
}

/// LandscapeLayerSample: the weight of its layer.
pub fn layer_sample(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let layer = x.text("ParameterName").unwrap_or("").to_string();
    Ok(layer_weight(l, &layer, &o))
}

/// LandscapeLayerWeight: `Base + Layer * weight`.
pub fn layer_weight_node(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let layer = x.text("ParameterName").unwrap_or("").to_string();
    let base = match l.input(f, x, "Base")? {
        Some(id) => id,
        None => {
            let c = x.numbers("ConstBase").unwrap_or_else(|| vec![0.0; 3]);
            l.kv(&c[..c.len().min(3)], &o)
        }
    };
    let value = l.required(f, x, "Layer")?;
    let weight = layer_weight(l, &layer, &o);
    let scaled = l.op2("multiply", value, weight, &o)?;
    l.op2("add", base, scaled, &o)
}

/// LandscapeLayerSwitch: Unreal picks a branch per component, by whether the component paints the layer. One material
/// is lowered for the whole landscape, so it takes `LayerUsed`: where the layer is not painted, its weight is 0 and
/// a layer branch built on that weight gives what the unused branch would.
pub fn layer_switch(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    match l.input(f, x, "LayerUsed")? {
        Some(id) => Ok(id),
        None => l.required(f, x, "LayerNotUsed"),
    }
}

/// LandscapeVisibilityMask: `1 - weight` of the visibility layer (holes), 1 where it is not painted.
pub fn visibility_mask(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let layer = x.text("ParameterName").unwrap_or("__LANDSCAPE_VISIBILITY__").to_string();
    let weight = layer_weight(l, &layer, &o);
    let one = l.k(1.0, &o);
    l.op2("subtract", one, weight, &o)
}

/// LandscapeLayerCoords: the landscape's layer coordinates (texture coordinate 0 for XY, 1 for XZ, YZ from both, or
/// the custom set), scaled by `1 / MappingScale`, rotated by `MappingRotation` degrees and panned.
pub fn layer_coords(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let custom = match x.text("CustomUVType").unwrap_or("LCCT_None") {
        "LCCT_CustomUV0" => Some(0),
        "LCCT_CustomUV1" => Some(1),
        "LCCT_CustomUV2" => Some(2),
        "LCCT_WeightMapUV" => Some(3),
        _ => None,
    };
    if let Some(index) = custom {
        return l.unreal_uv(index, &o);
    }
    let base = match x.text("MappingType").unwrap_or("TCMT_Auto") {
        "TCMT_XZ" => l.unreal_uv(1, &o)?,
        "TCMT_YZ" => {
            let (xy, xz) = (l.unreal_uv(0, &o)?, l.unreal_uv(1, &o)?);
            let (y, z) = (l.channel(xy, 1, &o)?, l.channel(xz, 1, &o)?);
            l.combine(&[y, z], &o)?
        }
        _ => l.unreal_uv(0, &o)?,
    };
    let mapping = x.number("MappingScale").unwrap_or(0.0);
    let scale = if mapping == 0.0 { 1.0 } else { 1.0 / mapping };
    let angle = x.number("MappingRotation").unwrap_or(0.0).to_radians();
    let (rx, ry) = (angle.cos() * scale, angle.sin() * scale);
    let a = l.kv(&[rx, ry], &o);
    let b = l.kv(&[-ry, rx], &o);
    let u = l.op("dotproduct", Type::Float, &[("in1", Arg::Node(base)), ("in2", Arg::Node(a))], &o)?;
    let v = l.op("dotproduct", Type::Float, &[("in1", Arg::Node(base)), ("in2", Arg::Node(b))], &o)?;
    let uv = l.combine(&[u, v], &o)?;
    let pan = l.kv(&[x.number("MappingPanU").unwrap_or(0.0), x.number("MappingPanV").unwrap_or(0.0)], &o);
    l.op2("add", uv, pan, &o)
}

/// One layer of a LandscapeLayerBlend, from its `Layers` property.
pub struct BlendLayer {
    pub name: String,
    pub blend: String,
    pub const_input: [f32; 3],
    pub const_height: f32,
}

/// The layers of a LandscapeLayerBlend, in order.
pub fn blend_layers(x: &Expression) -> Vec<BlendLayer> {
    let Some(layers) = x.properties.get("Layers").and_then(|v| v.as_array()) else { return Vec::new() };
    layers
        .iter()
        .map(|layer| {
            let number = |k: &str| layer.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let input = layer.get("ConstLayerInput").and_then(|v| v.as_array());
            let c = |i: usize| input.and_then(|a| a.get(i)).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            BlendLayer {
                name: layer.get("LayerName").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                blend: layer.get("BlendType").and_then(|v| v.as_str()).unwrap_or("LB_WeightBlend").to_string(),
                const_input: [c(0), c(1), c(2)],
                const_height: number("ConstHeightInput"),
            }
        })
        .collect()
}

/// The weights a LandscapeLayerBlend applies: per layer (in order), its weight, or for an alpha-blended layer its
/// alpha; and whether the weight-blended ones are divided by their sum (any height blend). A height-blended layer's
/// weight is `clamp(lerp(-1, 1, w) + height, 0.0001, 1)`.
pub struct BlendWeights {
    pub weights: Vec<NodeId>,
    pub alpha: Vec<bool>,
    pub inverse_sum: Option<NodeId>,
}

pub fn blend_weights(l: &mut Lowerer, f: usize, x: &Expression, layers: &[BlendLayer]) -> Result<BlendWeights, Error> {
    let o = l.origin(f, x);
    let mut weights = Vec::with_capacity(layers.len());
    let mut alpha = Vec::with_capacity(layers.len());
    let mut sum: Option<NodeId> = None;
    let mut renormalize = false;
    for layer in layers {
        let w = layer_weight(l, &layer.name, &o);
        let is_alpha = layer.blend == "LB_AlphaBlend";
        let w = if layer.blend == "LB_HeightBlend" {
            renormalize = true;
            let height = l.input_or(f, x, &format!("Height {}", layer.name), "", layer.const_height)?;
            let (minus, one) = (l.k(-1.0, &o), l.k(1.0, &o));
            let lerp = l.op(
                "mix",
                Type::Float,
                &[("fg", Arg::Node(one)), ("bg", Arg::Node(minus)), ("mix", Arg::Node(w))],
                &o,
            )?;
            let raised = l.op2("add", lerp, height, &o)?;
            let low = l.k(0.0001, &o);
            let raised = l.op2("max", raised, low, &o)?;
            l.op2("min", raised, one, &o)?
        } else {
            w
        };
        if !is_alpha {
            sum = Some(match sum {
                Some(s) => l.op2("add", s, w, &o)?,
                None => w,
            });
        }
        weights.push(w);
        alpha.push(is_alpha);
    }
    let inverse_sum = match (renormalize, sum) {
        (true, Some(s)) => {
            let one = l.k(1.0, &o);
            Some(l.op2("divide", one, s, &o)?)
        }
        _ => None,
    };
    Ok(BlendWeights { weights, alpha, inverse_sum })
}

/// `base + Σ (value_i - base) w_i` (times the inverse weight sum when renormalized) over the weight- and
/// height-blended layers, then `lerp(out, value_i, w_i)` for the alpha-blended ones in order. `values[i]` is `None`
/// for a layer that gives nothing for this output, which then leaves it out.
pub fn blend_values(
    l: &mut Lowerer,
    o: &str,
    base: NodeId,
    values: &[Option<NodeId>],
    weights: &BlendWeights,
    relative: bool,
) -> Result<NodeId, Error> {
    let mut out = base;
    for (i, value) in values.iter().enumerate() {
        let Some(value) = *value else { continue };
        if weights.alpha[i] {
            continue;
        }
        let contribution = if relative { l.op2("subtract", value, base, o)? } else { value };
        let w = match weights.inverse_sum {
            Some(inverse) => l.op2("multiply", weights.weights[i], inverse, o)?,
            None => weights.weights[i],
        };
        let term = l.op2("multiply", contribution, w, o)?;
        out = l.op2("add", out, term, o)?;
    }
    for (i, value) in values.iter().enumerate() {
        let Some(value) = *value else { continue };
        if !weights.alpha[i] {
            continue;
        }
        let (value, current) = l.widen(value, out, o)?;
        let ty = l.e.dag.node(value).ty;
        out = l.op(
            "mix",
            ty,
            &[("fg", Arg::Node(value)), ("bg", Arg::Node(current)), ("mix", Arg::Node(weights.weights[i]))],
            o,
        )?;
    }
    Ok(out)
}

/// LandscapeLayerBlend as a value: Unreal blends against zero.
pub fn layer_blend(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let layers = blend_layers(x);
    let weights = blend_weights(l, f, x, &layers)?;
    let mut values = Vec::with_capacity(layers.len());
    for layer in &layers {
        let value = match l.input(f, x, &format!("Layer {}", layer.name))? {
            Some(id) => id,
            None => l.kv(&layer.const_input, &o),
        };
        values.push(Some(value));
    }
    let zero = l.k(0.0, &o);
    blend_values(l, &o, zero, &values, &weights, false)
}

/// LandscapeGrassOutput: the grass Unreal scatters on the landscape at run time (by density per grass type), not
/// part of the surface; read as 0.
pub fn grass_output(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    l.warn(f, x, "landscape grass is scattered by Unreal at run time, not part of the surface");
    Ok(l.k(0.0, &l.origin(f, x)))
}
