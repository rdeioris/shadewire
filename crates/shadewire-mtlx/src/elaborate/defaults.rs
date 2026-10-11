//! Nodedef defaults: a geometric property read, the declared value, or zero.

use shadewire_ir::{ContextRead, Error, Location, Node, NodeId, NodeKind, Space, Type, Value};

use super::Elaborator;
use crate::document::{GeomPropDef, PortDef, PortType};
use crate::index::Index;

impl<'a> Elaborator<'a> {
    /// What an input not given on the node reads: `defaultgeomprop`, else the
    /// nodedef value, else zero.
    pub fn nodedef_default(&mut self, port: &PortDef, ty: Type, at: &Location) -> Result<NodeId, Error> {
        if let Some(name) = &port.default_geomprop {
            return self.geomprop_node(name, ty, at);
        }
        let value = match port.value.as_deref() {
            Some(text) if !text.trim().is_empty() => {
                Value::parse(ty, text).map_err(|e| Error::BadValue { at: at.clone(), message: e.to_string() })?
            }
            _ => Value::zero(ty),
        };
        Ok(self.dag.add(Node { kind: NodeKind::Const(value), ty, origin: at.path.clone() }))
    }

    /// A context node reading the geompropdef `name`, typed `ty` (the reading port's type).
    pub fn geomprop_node(&mut self, name: &str, ty: Type, at: &Location) -> Result<NodeId, Error> {
        let read =
            geomprop_read(&self.index, name, ty).map_err(|message| Error::BadValue { at: at.clone(), message })?;
        let read_ty = read.ty();
        Ok(self.dag.add(Node { kind: NodeKind::Context(read), ty: read_ty, origin: at.path.clone() }))
    }
}

/// The host read a geompropdef names. `ty` types the reads whose type is not
/// fixed (texcoord, geomcolor, custom properties) when the def has no value type.
pub fn geomprop_read(index: &Index, name: &str, ty: Type) -> Result<ContextRead, String> {
    let def = index.geompropdef(name).ok_or_else(|| format!("no geompropdef named {name}"))?;
    let space = || def_space(def);
    let index_or_zero = def.index.unwrap_or(0);
    let ty = match def.port_type {
        PortType::Value(t) => t,
        _ => ty,
    };
    Ok(match def.geomprop.as_str() {
        "position" => ContextRead::Position(space()?),
        "normal" => ContextRead::Normal(space()?),
        "tangent" => ContextRead::Tangent(space()?),
        "bitangent" => ContextRead::Bitangent(space()?),
        "texcoord" => ContextRead::TexCoord { index: index_or_zero, ty },
        "geomcolor" => ContextRead::GeomColor { index: index_or_zero, ty },
        other => ContextRead::GeomProp { name: other.to_string(), ty },
    })
}

/// The space of a position/normal/tangent/bitangent def; `object` when not given.
fn def_space(def: &GeomPropDef) -> Result<Space, String> {
    let name = def.space.as_deref().unwrap_or("object");
    Space::from_mtlx(name).ok_or_else(|| format!("unknown space {name} on geompropdef {}", def.name))
}

#[cfg(test)]
mod tests {
    use super::geomprop_read;
    use crate::document::Document;
    use crate::document::build::build;
    use crate::index::Index;
    use crate::library;
    use crate::parse::parse_str;
    use shadewire_ir::{ContextRead, Space, Type};

    fn user() -> Document {
        let text = r#"<materialx version="1.39">
          <geompropdef name="Cd" type="color3" geomprop="geomcolor" index="1"/>
          <geompropdef name="wear" type="float" geomprop="wear"/>
        </materialx>"#;
        build(&parse_str(text, None).unwrap(), None).unwrap()
    }

    #[test]
    fn stdlib_defs_map_to_their_reads() {
        let doc = user();
        let index = Index::new(&doc, library::standard());
        assert_eq!(geomprop_read(&index, "Nworld", Type::Vector3), Ok(ContextRead::Normal(Space::World)));
        assert_eq!(geomprop_read(&index, "Pobject", Type::Vector3), Ok(ContextRead::Position(Space::Object)));
        assert_eq!(geomprop_read(&index, "Tworld", Type::Vector3), Ok(ContextRead::Tangent(Space::World)));
        assert_eq!(
            geomprop_read(&index, "UV0", Type::Vector2),
            Ok(ContextRead::TexCoord { index: 0, ty: Type::Vector2 })
        );
        assert_eq!(
            geomprop_read(&index, "Cd", Type::Color3),
            Ok(ContextRead::GeomColor { index: 1, ty: Type::Color3 })
        );
        assert_eq!(
            geomprop_read(&index, "wear", Type::Float),
            Ok(ContextRead::GeomProp { name: "wear".into(), ty: Type::Float })
        );
        assert!(geomprop_read(&index, "nope", Type::Float).is_err());
    }
}
