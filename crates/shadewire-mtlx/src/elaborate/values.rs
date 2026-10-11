//! Values authored in the document. Task 17 makes eligible ones uniforms.

use shadewire_ir::{Error, Location, Node, NodeId, NodeKind, Type, Value};

use super::{Elaborator, Scope};

impl<'a> Elaborator<'a> {
    /// An authored value string as a node. For now always a `Const`.
    pub fn authored_value(
        &mut self,
        _scope: &Scope<'a>,
        _port_name: &str,
        ty: Type,
        text: &str,
        _uniform_eligible: bool,
        at: &Location,
    ) -> Result<NodeId, Error> {
        let value = Value::parse(ty, text).map_err(|e| Error::BadValue { at: at.clone(), message: e.to_string() })?;
        Ok(self.dag.add(Node { kind: NodeKind::Const(value), ty, origin: at.path.clone() }))
    }
}
