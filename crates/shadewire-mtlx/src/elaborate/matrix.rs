//! `transformmatrix` of a `vector3` by a constant `matrix33`, as three dot
//! products. No matrix enters the IR: the matrix must be authored on the
//! node or come from a sibling `constant`, as in the cmlib colorspace graphs.

use shadewire_ir::{Error, Location, Node as IrNode, NodeId, NodeKind, Op, Type, Value};

use super::{Elaborator, Scope};
use crate::document::{Node, NodeDef, PortType};

impl<'a> Elaborator<'a> {
    /// `in * mat` in the MaterialX (row-vector) convention: output channel
    /// `i` is `dot(in, (m[i], m[3 + i], m[6 + i]))`.
    pub fn elaborate_transformmatrix(
        &mut self,
        scope: &Scope<'a>,
        node: &'a Node,
        nd: &'a NodeDef,
        out: Type,
        at: &Location,
    ) -> Result<NodeId, Error> {
        let mat_type = nd.inputs.iter().find(|p| p.name == "mat").map(|p| &p.port_type);
        if out != Type::Vector3 || mat_type != Some(&PortType::Value(Type::Matrix33)) {
            return Err(Error::UnsupportedType { at: at.clone(), ty: format!("{} transformmatrix", nd.name) });
        }
        let m = self.constant_matrix33(scope, node, nd, at)?;
        let v = self.input_value(scope, node, nd, "in")?;
        let mut channels = Vec::new();
        for i in 0..3 {
            let column = self.add_at(NodeKind::Const(Value::Vector3([m[i], m[3 + i], m[6 + i]])), Type::Vector3, at);
            channels.push(self.add_at(NodeKind::Op { op: Op::DotProduct, args: vec![v, column] }, Type::Float, at));
        }
        Ok(self.add_at(NodeKind::Op { op: Op::Combine, args: channels }, Type::Vector3, at))
    }

    /// The `mat` input: its authored value, a sibling `constant` node's
    /// authored value, or the nodedef default. Anything else is unsupported.
    fn constant_matrix33(
        &mut self,
        scope: &Scope<'a>,
        node: &'a Node,
        nd: &'a NodeDef,
        at: &Location,
    ) -> Result<[f32; 9], Error> {
        let unsupported = || Error::UnsupportedType { at: at.clone(), ty: "matrix33".to_string() };
        let given = node.inputs.iter().find(|i| i.name == "mat");
        let text = match given {
            Some(input) if input.value.is_some() => input.value.clone(),
            Some(input) => match input.nodename.as_deref().and_then(|name| scope.find_node(name)) {
                Some(source) if source.category == "constant" => {
                    source.inputs.iter().find(|i| i.name == "value").and_then(|i| i.value.clone())
                }
                _ => return Err(unsupported()),
            },
            None => nd.inputs.iter().find(|p| p.name == "mat").and_then(|p| p.value.clone()),
        };
        let text = text.ok_or_else(unsupported)?;
        match Value::parse(Type::Matrix33, &text) {
            Ok(Value::Matrix33(m)) => Ok(m),
            Ok(_) => Err(unsupported()),
            Err(e) => Err(Error::BadValue { at: at.clone(), message: e.to_string() }),
        }
    }

    fn add_at(&mut self, kind: NodeKind, ty: Type, at: &Location) -> NodeId {
        self.dag.add(IrNode { kind, ty, origin: at.path.clone() })
    }
}
