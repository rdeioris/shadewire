//! Values authored in the document: constants, or uniforms when eligible.

use shadewire::{Error, Location, Node, NodeId, NodeKind, Type, UiHints, Value};

use super::Elaborator;
use crate::document::UiAttrs;

impl<'a> Elaborator<'a> {
    /// An authored value string as a node. Eligible values (surface-node
    /// inputs, top-level nodegraph interface values) become uniforms named
    /// after their port when `values_as_uniforms` is on; everything else,
    /// and every value read as an op parameter, is a `Const`. A uniform keeps `ui`.
    pub fn authored_value(
        &mut self,
        port_name: &str,
        ty: Type,
        text: &str,
        ui: &UiAttrs,
        uniform_eligible: bool,
        at: &Location,
    ) -> Result<NodeId, Error> {
        let value = Value::parse(ty, text).map_err(|e| Error::BadValue { at: at.clone(), message: e.to_string() })?;
        let kind = if uniform_eligible && self.options.values_as_uniforms {
            let name = self.unique_uniform_name(port_name);
            let hints = ui_hints(ty, ui);
            if !hints.is_empty() {
                self.dag.uniform_ui.insert(name.clone(), hints);
            }
            NodeKind::Uniform { name, value }
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

/// Limits that do not parse as `ty` are dropped: ui attributes only advise.
fn ui_hints(ty: Type, ui: &UiAttrs) -> UiHints {
    let limit = |text: &Option<String>| text.as_deref().and_then(|t| Value::parse(ty, t).ok());
    UiHints {
        min: limit(&ui.min),
        max: limit(&ui.max),
        soft_min: limit(&ui.soft_min),
        soft_max: limit(&ui.soft_max),
        folder: ui.folder.clone(),
    }
}
