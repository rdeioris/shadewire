//! `ramplr` and `ramptb`: in 1.39 these are GLSL functions, not library
//! graphs, so they are spelled out here as the ops they are:
//! `mix(low, high, clamp(texcoord.<x|y>, 0, 1))`.

use shadewire::{Error, Location, Node as IrNode, NodeId, NodeKind, Op, Type, Value};

use super::{Elaborator, Scope};
use crate::document::{Node, NodeDef};

/// True for the categories `elaborate_ramp` handles.
pub fn is_ramp(category: &str) -> bool {
    matches!(category, "ramplr" | "ramptb")
}

impl<'a> Elaborator<'a> {
    pub fn elaborate_ramp(
        &mut self,
        scope: &Scope<'a>,
        node: &'a Node,
        nd: &'a NodeDef,
        out: Type,
        at: &Location,
    ) -> Result<NodeId, Error> {
        // mx_ramplr: mix(valuel, valuer, clamp(texcoord.x)); mx_ramptb: mix(valueb, valuet, clamp(texcoord.y)).
        let (low, high, channel) = match node.category.as_str() {
            "ramplr" => ("valuel", "valuer", 0),
            _ => ("valueb", "valuet", 1),
        };
        let low = self.input_value(scope, node, nd, low)?;
        let high = self.input_value(scope, node, nd, high)?;
        let texcoord = self.input_value(scope, node, nd, "texcoord")?;
        let origin = at.path.clone();
        let mut add = |kind: NodeKind, ty: Type| self.dag.add(IrNode { kind, ty, origin: origin.clone() });
        let coord = add(NodeKind::Op { op: Op::Extract(channel), args: vec![texcoord] }, Type::Float);
        let zero = add(NodeKind::Const(Value::Float(0.0)), Type::Float);
        let one = add(NodeKind::Const(Value::Float(1.0)), Type::Float);
        let t = add(NodeKind::Op { op: Op::Clamp, args: vec![coord, zero, one] }, Type::Float);
        Ok(add(NodeKind::Op { op: Op::Mix, args: vec![high, low, t] }, out))
    }
}

#[cfg(test)]
mod tests {
    use shadewire::interpret::{ContextSource, NoTextures, eval_surface};
    use shadewire::{ContextRead, Space, TransformKind, Value};

    use super::super::elaborate_output;
    use super::super::tests::doc;

    struct NoContext;
    impl ContextSource for NoContext {
        fn read(&mut self, read: &ContextRead) -> Value {
            panic!("unexpected read {read:?}")
        }
        fn transform(&mut self, _: TransformKind, _: Space, _: Space, v: [f32; 3]) -> [f32; 3] {
            v
        }
    }

    /// Evaluates ramp `node` at texture coordinate `uv`, fed through a connected constant: an authored
    /// `texcoord` value is ignored in favour of its `defaultgeomprop`, as in MaterialX.
    fn ramp(node: &str, uv: &str) -> Value {
        let d = doc(&format!(
            r#"<nodegraph name="g"><constant name="uv" type="vector2"><input name="value" type="vector2" value="{uv}"/></constant>
               {node}<output name="out" type="color3" nodename="n"/></nodegraph>"#
        ));
        let dag = elaborate_output(&d, "g", "out").unwrap();
        eval_surface(&dag, &mut NoContext, &NoTextures).remove(0).1
    }

    #[test]
    fn ramplr_mixes_left_to_right_along_u() {
        let v = ramp(
            r#"<ramplr name="n" type="color3"><input name="valuel" type="color3" value="1,0,0"/>
               <input name="valuer" type="color3" value="0,1,0"/><input name="texcoord" type="vector2" nodename="uv"/></ramplr>"#,
            "0.25,0.8",
        );
        assert_eq!(v, Value::Color3([0.75, 0.25, 0.0]));
    }

    #[test]
    fn ramptb_mixes_bottom_to_top_along_v() {
        let v = ramp(
            r#"<ramptb name="n" type="color3"><input name="valuet" type="color3" value="1,0,0"/>
               <input name="valueb" type="color3" value="0,0,1"/><input name="texcoord" type="vector2" nodename="uv"/></ramptb>"#,
            "0.25,0.75",
        );
        assert_eq!(v, Value::Color3([0.75, 0.0, 0.25]));
    }

    #[test]
    fn ramps_clamp_the_coordinate() {
        let v = ramp(
            r#"<ramplr name="n" type="color3"><input name="valuel" type="color3" value="1,0,0"/>
               <input name="valuer" type="color3" value="0,1,0"/><input name="texcoord" type="vector2" nodename="uv"/></ramplr>"#,
            "1.5,0",
        );
        assert_eq!(v, Value::Color3([0.0, 1.0, 0.0]));
    }
}
