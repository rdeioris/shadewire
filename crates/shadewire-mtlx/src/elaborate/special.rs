//! Categories with no op and no implementation graph: pass-throughs,
//! channel splits, host reads and `image`.

use shadewire_ir::{ContextRead, Error, Location, Node as IrNode, NodeId, NodeKind, Op, Space, Type, Value};

use super::{Elaborator, Scope};
use crate::document::{Node, NodeDef};

/// True for categories `elaborate_special` handles. Reached only when no
/// nodegraph implements the resolved nodedef.
pub fn is_special(category: &str) -> bool {
    matches!(
        category,
        "constant"
            | "dot"
            | "convert"
            | "separate2"
            | "separate3"
            | "separate4"
            | "position"
            | "normal"
            | "tangent"
            | "bitangent"
            | "texcoord"
            | "geomcolor"
            | "geompropvalue"
            | "viewdirection"
            | "time"
            | "frame"
            | "image"
            | "transformmatrix"
    )
}

impl<'a> Elaborator<'a> {
    pub fn elaborate_special(
        &mut self,
        scope: &Scope<'a>,
        node: &'a Node,
        nd: &'a NodeDef,
        output: Option<&str>,
    ) -> Result<NodeId, Error> {
        let at = self.at(node.line, scope.origin(&node.name));
        let out = self.output_type(nd, output, &at)?;
        let read = match node.category.as_str() {
            "constant" => return self.input_value(scope, node, nd, "value"),
            "dot" => return self.input_value(scope, node, nd, "in"),
            "convert" => return self.leaf(scope, node, nd, Op::Convert, out, &at),
            "separate2" | "separate3" | "separate4" => {
                // Not `leaf`: its extract reads an `index` input, separate has none.
                let op = Op::Extract(separate_index(nd, output, &at)?);
                let args = vec![self.input_value(scope, node, nd, "in")?];
                return Ok(self.dag.add(IrNode { kind: NodeKind::Op { op, args }, ty: out, origin: at.path.clone() }));
            }
            "image" => return self.elaborate_image(scope, node, nd, out, &at),
            "transformmatrix" => return self.elaborate_transformmatrix(scope, node, nd, out, &at),
            "position" => ContextRead::Position(self.read_space(scope, node, nd, "object", &at)?),
            "normal" => ContextRead::Normal(self.read_space(scope, node, nd, "object", &at)?),
            "tangent" => ContextRead::Tangent(self.read_space(scope, node, nd, "object", &at)?),
            "bitangent" => ContextRead::Bitangent(self.read_space(scope, node, nd, "object", &at)?),
            "viewdirection" => ContextRead::ViewDirection(self.read_space(scope, node, nd, "world", &at)?),
            "texcoord" => ContextRead::TexCoord { index: self.read_index(scope, node, nd, &at)?, ty: out },
            "geomcolor" => ContextRead::GeomColor { index: self.read_index(scope, node, nd, &at)?, ty: out },
            "geompropvalue" => self.geompropvalue(scope, node, nd, out, &at)?,
            "time" => ContextRead::Time,
            "frame" => ContextRead::Frame,
            other => return Err(Error::MissingLeafImpl { at, nodedef: format!("{} ({other})", nd.name) }),
        };
        let ty = read.ty();
        Ok(self.dag.add(IrNode { kind: NodeKind::Context(read), ty, origin: at.path.clone() }))
    }

    /// The `space` string input, `default` when absent or empty.
    fn read_space(&mut self, scope: &Scope<'a>, node: &'a Node, nd: &'a NodeDef, default: &str, at: &Location) -> Result<Space, Error> {
        let given = self.string_input(scope, node, nd, "space")?.unwrap_or_default();
        let name = if given.is_empty() { default.to_string() } else { given };
        Space::from_mtlx(&name).ok_or_else(|| Error::BadValue { at: at.clone(), message: format!("unknown space {name}") })
    }

    /// The constant `index` integer input; 0 when the nodedef has none.
    fn read_index(&mut self, scope: &Scope<'a>, node: &'a Node, nd: &'a NodeDef, at: &Location) -> Result<u32, Error> {
        if !nd.inputs.iter().any(|p| p.name == "index") {
            return Ok(0);
        }
        match self.constant_input(scope, node, nd, "index", at)? {
            Value::Integer(i) if i >= 0 => Ok(i as u32),
            other => Err(Error::BadValue { at: at.clone(), message: format!("index must be a non-negative integer, found {other:?}") }),
        }
    }

    /// A named custom property read; its `default` input is recorded for reflection.
    fn geompropvalue(&mut self, scope: &Scope<'a>, node: &'a Node, nd: &'a NodeDef, ty: Type, at: &Location) -> Result<ContextRead, Error> {
        let name = self.string_input(scope, node, nd, "geomprop")?.unwrap_or_default();
        if name.is_empty() {
            return Err(Error::BadValue { at: at.clone(), message: "geompropvalue names no geomprop".to_string() });
        }
        if nd.inputs.iter().any(|p| p.name == "default") {
            let default = self.constant_input(scope, node, nd, "default", at)?;
            self.geomprop_defaults.insert(name.clone(), default);
        }
        Ok(ContextRead::GeomProp { name, ty })
    }

    /// A value input that must be known at compile time. It is read as a
    /// parameter, so it never becomes a uniform; only a constant is accepted.
    pub fn constant_input(&mut self, scope: &Scope<'a>, node: &'a Node, nd: &'a NodeDef, name: &str, at: &Location) -> Result<Value, Error> {
        let id = self.param_value(scope, node, nd, name)?;
        match &self.dag.node(id).kind {
            NodeKind::Const(value) => Ok(value.clone()),
            other => Err(Error::BadValue { at: at.clone(), message: format!("{name} must be a constant, found {other:?}") }),
        }
    }
}

/// The position of the requested output among the nodedef's outputs:
/// `outg` of `separate3` is channel 1.
fn separate_index(nd: &NodeDef, output: Option<&str>, at: &Location) -> Result<u32, Error> {
    let name = output.unwrap_or("");
    match nd.outputs.iter().position(|o| o.name == name) {
        Some(index) => Ok(index as u32),
        None => Err(Error::BadValue { at: at.clone(), message: format!("{} has no output {name}", nd.name) }),
    }
}
