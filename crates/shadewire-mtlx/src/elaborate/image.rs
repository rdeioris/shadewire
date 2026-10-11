//! The `image` node: a texture lookup through `Dag::textures`.

use shadewire_ir::{AddressMode, Error, FilterMode, Location, Node as IrNode, NodeId, NodeKind, Op, TextureDesc, Type};

use super::{Elaborator, Scope};
use crate::document::{Node, NodeDef};

impl<'a> Elaborator<'a> {
    /// `file` (required), `default`, `texcoord`, address and filter modes, and
    /// `uvtiling`/`uvoffset` when the nodedef has them. `layer` and the frame
    /// inputs are ignored. The result is converted from the file's colorspace.
    pub fn elaborate_image(&mut self, scope: &Scope<'a>, node: &'a Node, nd: &'a NodeDef, ty: Type, at: &Location) -> Result<NodeId, Error> {
        let file = match self.text_input(scope, node, nd, "file")? {
            Some(file) if !file.text.is_empty() => file,
            _ => return Err(Error::BadValue { at: at.clone(), message: "image has no file".to_string() }),
        };
        let address_u = self.address_mode(scope, node, nd, "uaddressmode", at)?;
        let address_v = self.address_mode(scope, node, nd, "vaddressmode", at)?;
        let filter_name = self.string_input(scope, node, nd, "filtertype")?.unwrap_or_default();
        let filter = match filter_name.as_str() {
            "" => FilterMode::Linear,
            name => FilterMode::from_mtlx(name)
                .ok_or_else(|| Error::BadValue { at: at.clone(), message: format!("unknown filtertype {name}") })?,
        };
        let colorspace = file.colorspace.clone();
        let texture = self.dag.add_texture(TextureDesc { file: file.text, colorspace: colorspace.clone(), address_u, address_v, filter });
        let default = self.constant_input(scope, node, nd, "default", at)?;
        let uv = self.image_uv(scope, node, nd, at)?;
        let sample = self.dag.add(IrNode { kind: NodeKind::Sample { texture, uv, default }, ty, origin: at.path.clone() });
        self.convert_sample_colorspace(sample, colorspace.as_deref(), at)
    }

    /// The texture coordinate, as `mx_transform_uv`: `texcoord * uvtiling + uvoffset`.
    fn image_uv(&mut self, scope: &Scope<'a>, node: &'a Node, nd: &'a NodeDef, at: &Location) -> Result<NodeId, Error> {
        let has = |name: &str| nd.inputs.iter().any(|p| p.name == name);
        let mut uv = self.input_value(scope, node, nd, "texcoord")?;
        if has("uvtiling") {
            let tiling = self.input_value(scope, node, nd, "uvtiling")?;
            uv = self.vector2_op(Op::Multiply, uv, tiling, at);
        }
        if has("uvoffset") {
            let offset = self.input_value(scope, node, nd, "uvoffset")?;
            uv = self.vector2_op(Op::Add, uv, offset, at);
        }
        Ok(uv)
    }

    fn vector2_op(&mut self, op: Op, a: NodeId, b: NodeId, at: &Location) -> NodeId {
        self.dag.add(IrNode { kind: NodeKind::Op { op, args: vec![a, b] }, ty: Type::Vector2, origin: at.path.clone() })
    }

    /// An address mode input; `periodic` when absent or empty.
    fn address_mode(&mut self, scope: &Scope<'a>, node: &'a Node, nd: &'a NodeDef, name: &str, at: &Location) -> Result<AddressMode, Error> {
        let text = self.string_input(scope, node, nd, name)?.unwrap_or_default();
        if text.is_empty() {
            return Ok(AddressMode::Periodic);
        }
        AddressMode::from_mtlx(&text).ok_or_else(|| Error::BadValue { at: at.clone(), message: format!("unknown {name} {text}") })
    }

    /// Converts a sample from its file's colorspace to the working one (Task 17). The identity for now.
    pub fn convert_sample_colorspace(&mut self, sample: NodeId, _colorspace: Option<&str>, _at: &Location) -> Result<NodeId, Error> {
        Ok(sample)
    }
}
