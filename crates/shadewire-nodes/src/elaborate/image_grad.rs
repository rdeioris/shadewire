//! `shadewire_image_grad`: an `image` lookup with explicit screen gradients, as the writer spells `Sample { grad }`.

use shadewire::{Error, Location, Node as IrNode, NodeId, NodeKind, Type};

use super::{Elaborator, Scope};
use crate::document::{Node, NodeDef};

impl<'a> Elaborator<'a> {
    /// `file` (required), `default`, `texcoord`, the gradients `dx`/`dy`, address and filter modes. The result is
    /// converted from the file's colorspace, as an `image` sample is.
    pub fn elaborate_image_grad(
        &mut self,
        scope: &Scope<'a>,
        node: &'a Node,
        nd: &'a NodeDef,
        ty: Type,
        at: &Location,
    ) -> Result<NodeId, Error> {
        let (texture, colorspace) = self.image_texture(scope, node, nd, at)?;
        let default = self.constant_input(scope, node, nd, "default", at)?;
        let uv = self.input_value(scope, node, nd, "texcoord")?;
        let dx = self.input_value(scope, node, nd, "dx")?;
        let dy = self.input_value(scope, node, nd, "dy")?;
        let sample = self.dag.add(IrNode {
            kind: NodeKind::Sample { texture, uv, default, grad: Some([dx, dy]) },
            ty,
            origin: at.path.clone(),
        });
        self.convert_sample_colorspace(sample, colorspace.as_deref(), at)
    }
}

#[cfg(test)]
mod tests {
    use shadewire::{Error, NodeKind, Op};

    use super::super::elaborate_output;
    use super::super::tests::doc;

    fn graph(tile: &str) -> String {
        format!(
            r#"<nodegraph name="g">
                 <texcoord name="uv" type="vector2"/>
                 <shadewire_dpdx name="dx" type="vector2"><input name="in" type="vector2" nodename="uv"/></shadewire_dpdx>
                 <shadewire_dpdy name="dy" type="vector2"><input name="in" type="vector2" nodename="uv"/></shadewire_dpdy>
                 <shadewire_hextile_coord name="c" type="vector2">
                   <input name="tile" type="integer" value="{tile}"/><input name="coord" type="vector2" nodename="uv"/>
                 </shadewire_hextile_coord>
                 <shadewire_image_grad name="i" type="color3">
                   <input name="file" type="filename" value="a.png"/>
                   <input name="texcoord" type="vector2" nodename="c"/>
                   <input name="dx" type="vector2" nodename="dx"/><input name="dy" type="vector2" nodename="dy"/>
                 </shadewire_image_grad>
                 <output name="out" type="color3" nodename="i"/>
               </nodegraph>"#
        )
    }

    #[test]
    fn a_gradient_sample_reads_its_dx_and_dy() {
        let dag = elaborate_output(&doc(&graph("2")), "g", "out").unwrap();
        let (uv, [dx, dy]) = dag
            .nodes
            .iter()
            .find_map(|n| match n.kind {
                NodeKind::Sample { uv, grad: Some(grad), .. } => Some((uv, grad)),
                _ => None,
            })
            .expect("no gradient sample");
        assert!(matches!(dag.node(dx).kind, NodeKind::Op { op: Op::Dpdx, .. }), "{:?}", dag.node(dx));
        assert!(matches!(dag.node(dy).kind, NodeKind::Op { op: Op::Dpdy, .. }), "{:?}", dag.node(dy));
        assert!(matches!(dag.node(uv).kind, NodeKind::Op { op: Op::HextileCoord(2), .. }), "{:?}", dag.node(uv));
    }

    #[test]
    fn a_tile_out_of_range_is_a_bad_value() {
        let err = elaborate_output(&doc(&graph("3")), "g", "out").unwrap_err();
        assert!(matches!(err, Error::BadValue { .. }), "{err:?}");
    }
}
