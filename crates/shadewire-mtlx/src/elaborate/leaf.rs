//! Leaf nodedefs: one op over the inputs the catalog names, in catalog order.

use shadewire_ir::{input_names, Error, Location, Node as IrNode, NodeId, NodeKind, Op, Space, Type, Value};

use super::{Elaborator, Scope};
use crate::document::{Node, NodeDef};

impl<'a> Elaborator<'a> {
    pub fn leaf(&mut self, scope: &Scope<'a>, node: &'a Node, nd: &'a NodeDef, op: Op, out: Type, at: &Location) -> Result<NodeId, Error> {
        let op = match op {
            Op::Extract(_) => Op::Extract(self.extract_index(scope, node, nd, at)?),
            Op::Transform { kind, .. } => {
                let from = self.space_input(scope, node, nd, "fromspace", at)?;
                let to = self.space_input(scope, node, nd, "tospace", at)?;
                match (from, to) {
                    (Some(from), Some(to)) => Op::Transform { kind, from, to },
                    // An empty space names no transform: the input passes through.
                    _ => return self.input_value(scope, node, nd, "in"),
                }
            }
            other => other,
        };
        let mut args = Vec::new();
        for name in input_names(&op) {
            if nd.inputs.iter().any(|p| p.name == *name) {
                args.push(self.input_value(scope, node, nd, name)?);
            }
        }
        Ok(self.dag.add(IrNode { kind: NodeKind::Op { op, args }, ty: out, origin: at.path.clone() }))
    }

    /// The constant integer `index` input of `extract`.
    fn extract_index(&mut self, scope: &Scope<'a>, node: &'a Node, nd: &'a NodeDef, at: &Location) -> Result<u32, Error> {
        let id = self.input_value(scope, node, nd, "index")?;
        match &self.dag.node(id).kind {
            NodeKind::Const(Value::Integer(i)) if *i >= 0 => Ok(*i as u32),
            other => Err(Error::BadValue { at: at.clone(), message: format!("extract index must be a constant integer, found {other:?}") }),
        }
    }

    /// A space string input. `None` when empty; an unknown name is a bad value.
    fn space_input(&mut self, scope: &Scope<'a>, node: &'a Node, nd: &'a NodeDef, name: &str, at: &Location) -> Result<Option<Space>, Error> {
        let text = self.string_input(scope, node, nd, name)?.unwrap_or_default();
        if text.is_empty() {
            return Ok(None);
        }
        Space::from_mtlx(&text)
            .map(Some)
            .ok_or_else(|| Error::BadValue { at: at.clone(), message: format!("unknown space {text} in {name}") })
    }
}
