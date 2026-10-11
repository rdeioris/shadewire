//! Values authored in the document: constants, or uniforms when eligible.

use shadewire_ir::{Error, Location, Node, NodeId, NodeKind, Type, Value};

use super::{Elaborator, Scope};

impl<'a> Elaborator<'a> {
    /// An authored value string as a node. Eligible values (surface-node
    /// inputs, top-level nodegraph interface values) become uniforms named
    /// after their port when `values_as_uniforms` is on; everything else,
    /// and every value read as an op parameter, is a `Const`.
    pub fn authored_value(
        &mut self,
        _scope: &Scope<'a>,
        port_name: &str,
        ty: Type,
        text: &str,
        uniform_eligible: bool,
        at: &Location,
    ) -> Result<NodeId, Error> {
        let value = Value::parse(ty, text).map_err(|e| Error::BadValue { at: at.clone(), message: e.to_string() })?;
        let kind = if uniform_eligible && self.options.values_as_uniforms {
            NodeKind::Uniform { name: self.unique_uniform_name(port_name), value }
        } else {
            NodeKind::Const(value)
        };
        Ok(self.dag.add(Node { kind, ty, origin: at.path.clone() }))
    }

    /// `base`, then `base_2`, `base_3`, ... for later uniforms of the same name.
    fn unique_uniform_name(&mut self, base: &str) -> String {
        let mut name = base.to_string();
        let mut n = 2;
        while self.uniform_names.contains(&name) {
            name = format!("{base}_{n}");
            n += 1;
        }
        self.uniform_names.insert(name.clone());
        name
    }
}
