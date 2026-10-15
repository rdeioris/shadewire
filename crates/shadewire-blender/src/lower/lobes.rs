//! The BSDF lobes of Blender's shader nodes as pbrlib lobes: the sockets each reads, and the pbrlib inputs they
//! become.

use shadewire::{Error, NodeId, Type, Value};
use shadewire_nodes::document::PortType;
use shadewire_nodes::{Arg, ShaderNode};

use super::Lowerer;
use super::values::socket_value;
use crate::graph::FlatNode;

/// The BSDF lobe of the Blender BSDF `node`; `None` for a kind that is not a lobe. The weight socket and the
/// distribution (GGX kinds) are ignored.
pub fn lobe(l: &mut Lowerer, node: &FlatNode) -> Result<Option<ShaderNode>, Error> {
    let mut inputs = Inputs::default();
    let category = match node.kind.as_str() {
        "ShaderNodeBsdfDiffuse" => {
            inputs.read(l, node, "Color", "color", Type::Color3)?;
            inputs.read(l, node, "Roughness", "roughness", Type::Float)?;
            inputs.read_if_linked(l, node, "Normal", "normal")?;
            "oren_nayar_diffuse_bsdf"
        }
        "ShaderNodeBsdfAnisotropic" => {
            l.warn(node, "tinted specular written as a Schlick lobe");
            let color = l.input(node, "Color", Type::Color3)?;
            inputs.put("color0", color);
            inputs.put("color82", color);
            inputs.roughness(l, node)?;
            inputs.read_if_linked(l, node, "Normal", "normal")?;
            inputs.read_if_linked(l, node, "Tangent", "tangent")?;
            dropped_if_set(l, node, "Rotation");
            "generalized_schlick_bsdf"
        }
        "ShaderNodeBsdfGlass" | "ShaderNodeBsdfRefraction" => {
            let mode = if node.kind == "ShaderNodeBsdfGlass" { "RT" } else { "T" };
            inputs.read(l, node, "Color", "tint", Type::Color3)?;
            inputs.read(l, node, "IOR", "ior", Type::Float)?;
            let roughness = l.input(node, "Roughness", Type::Float)?;
            let isotropic = l.float(0.0, &format!("{}.anisotropy", node.name));
            inputs.put("roughness", rough2(l, node, roughness, isotropic)?);
            inputs.read_if_linked(l, node, "Normal", "normal")?;
            inputs.0.push(("scatter_mode".to_string(), Arg::Text(mode.to_string())));
            dropped_if_set(l, node, "Thin Film Thickness");
            "dielectric_bsdf"
        }
        "ShaderNodeBsdfTranslucent" => {
            inputs.read(l, node, "Color", "color", Type::Color3)?;
            inputs.read_if_linked(l, node, "Normal", "normal")?;
            "translucent_bsdf"
        }
        "ShaderNodeBsdfSheen" => {
            inputs.read(l, node, "Color", "color", Type::Color3)?;
            inputs.read(l, node, "Roughness", "roughness", Type::Float)?;
            inputs.read_if_linked(l, node, "Normal", "normal")?;
            let mode = if node.text("distribution") == Some("MICROFIBER") { "zeltner" } else { "conty_kulla" };
            inputs.0.push(("mode".to_string(), Arg::Text(mode.to_string())));
            "sheen_bsdf"
        }
        "ShaderNodeSubsurfaceScattering" => {
            inputs.read(l, node, "Color", "color", Type::Color3)?;
            let radius = l.input(node, "Radius", Type::Vector3)?;
            let scale = l.input(node, "Scale", Type::Float)?;
            let origin = format!("{}.radius", node.name);
            let scaled =
                l.op("multiply", Type::Vector3, &[("in1", Arg::Node(radius)), ("in2", Arg::Node(scale))], &origin)?;
            inputs.put("radius", l.convert(scaled, Type::Color3, &origin)?);
            inputs.read(l, node, "Anisotropy", "anisotropy", Type::Float)?;
            inputs.read_if_linked(l, node, "Normal", "normal")?;
            "subsurface_bsdf"
        }
        "ShaderNodeBsdfMetallic" => {
            let conductor = node.text("fresnel_type") == Some("PHYSICAL_CONDUCTOR");
            if conductor {
                inputs.read(l, node, "IOR", "ior", Type::Color3)?;
                inputs.read(l, node, "Extinction", "extinction", Type::Color3)?;
            } else {
                inputs.read(l, node, "Base Color", "color0", Type::Color3)?;
                inputs.read(l, node, "Edge Tint", "color82", Type::Color3)?;
                inputs.0.push(("color90".to_string(), Arg::Value(Value::Color3([1.0, 1.0, 1.0]))));
                inputs.0.push(("exponent".to_string(), Arg::Value(Value::Float(5.0))));
            }
            inputs.roughness(l, node)?;
            inputs.read_if_linked(l, node, "Normal", "normal")?;
            inputs.read_if_linked(l, node, "Tangent", "tangent")?;
            dropped_if_set(l, node, "Rotation");
            dropped_if_set(l, node, "Thin Film Thickness");
            if conductor { "conductor_bsdf" } else { "generalized_schlick_bsdf" }
        }
        _ => return Ok(None),
    };
    Ok(Some(ShaderNode {
        name: node.name.clone(),
        category: category.to_string(),
        port_type: PortType::Closure("BSDF".to_string()),
        inputs: inputs.0,
        links: Vec::new(),
    }))
}

/// The pbrlib inputs of a lobe, in order.
#[derive(Default)]
struct Inputs(Vec<(String, Arg)>);

impl Inputs {
    fn put(&mut self, name: &str, id: NodeId) {
        self.0.push((name.to_string(), Arg::Node(id)));
    }

    /// Input `input` bound to `socket` of `node` read as `ty`.
    fn read(&mut self, l: &mut Lowerer, node: &FlatNode, socket: &str, input: &str, ty: Type) -> Result<(), Error> {
        let id = l.input(node, socket, ty)?;
        self.put(input, id);
        Ok(())
    }

    /// Input `input` (a vector) bound to `socket` only when something is linked to it, so the nodedef's default
    /// (the shading normal or tangent) stays otherwise.
    fn read_if_linked(&mut self, l: &mut Lowerer, node: &FlatNode, socket: &str, input: &str) -> Result<(), Error> {
        if let Some(id) = l.input_if_linked(node, socket, Type::Vector3)? {
            self.put(input, id);
        }
        Ok(())
    }

    /// The `roughness` input from the Roughness and Anisotropy sockets.
    fn roughness(&mut self, l: &mut Lowerer, node: &FlatNode) -> Result<(), Error> {
        let roughness = l.input(node, "Roughness", Type::Float)?;
        let anisotropy = l.input(node, "Anisotropy", Type::Float)?;
        let id = rough2(l, node, roughness, anisotropy)?;
        self.put("roughness", id);
        Ok(())
    }
}

/// The roughness vector2 of a scalar roughness and an anisotropy.
fn rough2(l: &mut Lowerer, node: &FlatNode, roughness: NodeId, anisotropy: NodeId) -> Result<NodeId, Error> {
    let inputs = [("roughness", Arg::Node(roughness)), ("anisotropy", Arg::Node(anisotropy))];
    l.op("roughness_anisotropy", Type::Vector2, &inputs, &format!("{}.roughness", node.name))
}

/// Warns that `socket` is dropped when it is linked or holds a non-zero value.
fn dropped_if_set(l: &mut Lowerer, node: &FlatNode, socket: &str) {
    let set = node.input(socket).and_then(socket_value).is_some_and(|v| v.as_f32() != 0.0);
    if l.linked(node, socket) || set {
        let what = if socket == "Rotation" { "Rotation" } else { "Thin Film" };
        l.warn(node, format!("{what} dropped"));
    }
}
