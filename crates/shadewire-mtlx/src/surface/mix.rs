//! Surface fields: a model's inputs, or a same-model mix of two surfaces.

use shadewire_ir::{Error, Node, NodeId, NodeKind, Op, SurfaceField, Type, Value};

use super::find::{MixInput, SurfaceSource};
use super::models::fields;
use crate::elaborate::Elaborator;

/// Approximation note: two surfaces are blended field by field, not by their lighting.
pub const SURFACE_MIX: &str = "surface_mix";

impl<'a> Elaborator<'a> {
    /// One field per value input of the model, in nodedef order. Values
    /// authored on a model node in the user's scope become uniforms.
    /// Each source is elaborated once, so a sub-mix shared by both sides of
    /// a mix makes its nodes and uniforms once.
    pub fn surface_fields(&mut self, source: &SurfaceSource<'a>) -> Result<Vec<SurfaceField>, Error> {
        let key = source.key();
        if let Some(fields) = self.surface_field_memo.get(&key) {
            return Ok(fields.clone());
        }
        let fields = self.elaborate_surface_fields(source)?;
        self.surface_field_memo.insert(key, fields.clone());
        Ok(fields)
    }

    fn elaborate_surface_fields(&mut self, source: &SurfaceSource<'a>) -> Result<Vec<SurfaceField>, Error> {
        match source {
            SurfaceSource::Model { node, nodedef, scope } => {
                let mut out = Vec::new();
                for (name, ty) in fields(nodedef) {
                    let id = self.surface_input_value(scope, node, nodedef, &name, scope.user)?;
                    out.push(SurfaceField { name, ty, node: id });
                }
                Ok(out)
            }
            SurfaceSource::Mix { fg, bg, mix_input } => self.mixed_fields(fg, bg, mix_input),
        }
    }

    /// Each field of `fg` and `bg` blended by the mix node's weight.
    fn mixed_fields(
        &mut self,
        fg: &SurfaceSource<'a>,
        bg: &SurfaceSource<'a>,
        mix: &MixInput<'a>,
    ) -> Result<Vec<SurfaceField>, Error> {
        let fg_fields = self.surface_fields(fg)?;
        let bg_fields = self.surface_fields(bg)?;
        if !sides_line_up(&fg_fields, &bg_fields) {
            let at = self.at(&mix.scope, mix.node.line, mix.scope.origin(&mix.node.name));
            return Err(Error::UnsupportedSurface {
                at,
                node: mix.node.name.clone(),
                reason: "mix sides differ".to_string(),
            });
        }
        let weight = self.surface_input_value(&mix.scope, mix.node, mix.nodedef, "mix", mix.scope.user)?;
        let origin = mix.scope.origin(&mix.node.name);
        let mut out = Vec::with_capacity(fg_fields.len());
        for (f, b) in fg_fields.into_iter().zip(bg_fields) {
            let node = self.mix_field(f.node, b.node, weight, f.ty, format!("{origin}.{}", f.name));
            out.push(SurfaceField { name: f.name, ty: f.ty, node });
        }
        if !self.dag.approximations.iter().any(|a| a == SURFACE_MIX) {
            self.dag.approximations.push(SURFACE_MIX.to_string());
        }
        Ok(out)
    }

    /// `mix(bg, fg, weight)`; booleans and integers cannot blend, so they
    /// take `fg` when the weight is at least one half.
    fn mix_field(&mut self, fg: NodeId, bg: NodeId, weight: NodeId, ty: Type, origin: String) -> NodeId {
        let kind = match ty {
            Type::Boolean | Type::Integer => {
                let half = Node { kind: NodeKind::Const(Value::Float(0.5)), ty: Type::Float, origin: origin.clone() };
                let half = self.dag.add(half);
                NodeKind::Op { op: Op::IfGreaterEq, args: vec![weight, half, fg, bg] }
            }
            _ => NodeKind::Op { op: Op::Mix, args: vec![fg, bg, weight] },
        };
        self.dag.add(Node { kind, ty, origin })
    }
}

/// Whether two surfaces' fields pair up by name and type, position by position.
pub fn sides_line_up(fg: &[SurfaceField], bg: &[SurfaceField]) -> bool {
    fg.len() == bg.len() && fg.iter().zip(bg).all(|(f, b)| f.name == b.name && f.ty == b.ty)
}

#[cfg(test)]
mod tests {
    use super::sides_line_up;
    use shadewire_ir::{NodeId, SurfaceField, Type};

    fn field(name: &str, ty: Type) -> SurfaceField {
        SurfaceField { name: name.to_string(), ty, node: NodeId(0) }
    }

    #[test]
    fn sides_must_pair_by_name_and_type() {
        let fg = [field("a", Type::Float), field("b", Type::Color3)];
        assert!(sides_line_up(&fg, &[field("a", Type::Float), field("b", Type::Color3)]));
        assert!(!sides_line_up(&fg, &[field("a", Type::Float)]));
        assert!(!sides_line_up(&fg, &[field("a", Type::Float), field("c", Type::Color3)]));
        assert!(!sides_line_up(&fg, &[field("a", Type::Float), field("b", Type::Vector3)]));
    }
}
