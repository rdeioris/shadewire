//! The Principled BSDF as `open_pbr_surface`: Blender's sockets to OpenPBR's fields, the inverse of what the
//! Blender import side does.

use shadewire::{Error, NodeId, Type};
use shadewire_nodes::Arg;

use super::Lowerer;
use super::surface::Shading;
use crate::graph::FlatNode;

/// OpenPBR fields read straight from one socket each: `(field, socket, type)`.
const DIRECT: [(&str, &str, Type); 23] = [
    ("base_metalness", "Metallic", Type::Float),
    ("specular_roughness", "Roughness", Type::Float),
    ("specular_ior", "IOR", Type::Float),
    ("geometry_opacity", "Alpha", Type::Float),
    ("geometry_thin_walled", "Thin Wall", Type::Boolean),
    ("base_diffuse_roughness", "Diffuse Roughness", Type::Float),
    ("subsurface_weight", "Subsurface Weight", Type::Float),
    ("subsurface_radius_scale", "Subsurface Radius", Type::Color3),
    ("subsurface_radius", "Subsurface Scale", Type::Float),
    ("subsurface_scatter_anisotropy", "Subsurface Anisotropy", Type::Float),
    ("specular_color", "Specular Tint", Type::Color3),
    ("specular_roughness_anisotropy", "Anisotropic", Type::Float),
    ("transmission_weight", "Transmission Weight", Type::Float),
    ("coat_weight", "Coat Weight", Type::Float),
    ("coat_roughness", "Coat Roughness", Type::Float),
    ("coat_ior", "Coat IOR", Type::Float),
    ("coat_color", "Coat Tint", Type::Color3),
    ("fuzz_weight", "Sheen Weight", Type::Float),
    ("fuzz_roughness", "Sheen Roughness", Type::Float),
    ("fuzz_color", "Sheen Tint", Type::Color3),
    ("emission_color", "Emission Color", Type::Color3),
    ("emission_luminance", "Emission Strength", Type::Float),
    ("thin_film_ior", "Thin Film IOR", Type::Float),
];

/// OpenPBR fields bound only when their socket is linked, so the nodedef's default (the shading normal or
/// tangent) stays otherwise: `(field, socket)`.
const LINKED: [(&str, &str); 3] =
    [("geometry_normal", "Normal"), ("geometry_tangent", "Tangent"), ("geometry_coat_normal", "Coat Normal")];

/// The `open_pbr_surface` fields of the Principled BSDF `node`. A socket the document does not have is not bound
/// (the nodedef's default), except Base Color, which three fields share.
pub fn fields(l: &mut Lowerer, node: &FlatNode) -> Result<Shading, Error> {
    let mut fields: Vec<(String, Arg)> = Vec::new();
    let mut bind = |field: &str, id: NodeId| fields.push((field.to_string(), Arg::Node(id)));
    let base = l.input(node, "Base Color", Type::Color3)?;
    bind("base_color", base);
    bind("subsurface_color", base);
    bind("transmission_color", base);
    for (field, socket, ty) in DIRECT {
        if let Some(id) = read(l, node, socket, ty)? {
            bind(field, id);
        }
    }
    for (field, socket) in LINKED {
        if let Some(id) = l.input_if_linked(node, socket, Type::Vector3)? {
            bind(field, id);
        }
    }
    let origin = |what: &str| format!("{}.{what}", node.name);
    if let Some(level) = read(l, node, "Specular IOR Level", Type::Float)? {
        let two = l.float(2.0, &origin("specular_weight"));
        let weight = l.op(
            "multiply",
            Type::Float,
            &[("in1", Arg::Node(level)), ("in2", Arg::Node(two))],
            &origin("specular_weight"),
        )?;
        bind("specular_weight", weight);
    }
    if let Some(nm) = read(l, node, "Thin Film Thickness", Type::Float)? {
        let per_nm = l.float(0.001, &origin("thin_film_thickness"));
        let thickness = l.op(
            "multiply",
            Type::Float,
            &[("in1", Arg::Node(nm)), ("in2", Arg::Node(per_nm))],
            &origin("thin_film_thickness"),
        )?;
        bind("thin_film_thickness", thickness);
        let (zero, one) = (l.float(0.0, &origin("thin_film_weight")), l.float(1.0, &origin("thin_film_weight")));
        let inputs =
            [("value1", Arg::Node(nm)), ("value2", Arg::Node(zero)), ("in1", Arg::Node(one)), ("in2", Arg::Node(zero))];
        bind("thin_film_weight", l.op("ifgreater", Type::Float, &inputs, &origin("thin_film_weight"))?);
    }
    let rotation = "Anisotropic Rotation";
    let rotated = node.input(rotation).and_then(super::values::socket_value).is_some_and(|v| v.as_f32() != 0.0);
    if l.linked(node, rotation) || rotated {
        l.warn(node, "Anisotropic Rotation dropped (OpenPBR rotates the tangent instead)");
    }
    Ok(Shading::Principled { fields, origin: node.name.clone() })
}

/// The socket as `ty` when the node has it or something is linked to it; `None` leaves the field unbound.
fn read(l: &mut Lowerer, node: &FlatNode, socket: &str, ty: Type) -> Result<Option<NodeId>, Error> {
    if node.input(socket).is_some() || l.linked(node, socket) { l.input(node, socket, ty).map(Some) } else { Ok(None) }
}
