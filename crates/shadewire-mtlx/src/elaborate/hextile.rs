//! `hextiledimage`: in 1.39 a GLSL function (`mx_hextiledimage.glsl`), not a library
//! graph, so it is spelled out here: three hex tiles (`HextileCoord`), each sampled with
//! its own gradients (`HextileGrad` of the coordinate's `Dpdx`/`Dpdy`, as `textureGrad`),
//! blended by `HextileWeights` over the samples' luminance.

use shadewire::{
    AddressMode, Error, FilterMode, Location, Node as IrNode, NodeId, NodeKind, Op, TextureDesc, Type, Value,
};

use super::{Elaborator, Scope};
use crate::document::{Node, NodeDef};

/// The per-tile inputs of `HextileCoord`, after the coordinate; `HextileGrad` takes the first four.
const TILE_INPUTS: [&str; 6] = ["rotation", "rotationrange", "scale", "scalerange", "offset", "offsetrange"];

impl<'a> Elaborator<'a> {
    /// The image is sampled periodic and linear (the nodedef has no mode inputs); the
    /// blended result is converted from the file's colorspace, as an `image` sample is.
    pub fn elaborate_hextiled_image(
        &mut self,
        scope: &Scope<'a>,
        node: &'a Node,
        nd: &'a NodeDef,
        ty: Type,
        at: &Location,
    ) -> Result<NodeId, Error> {
        let file = match self.text_input(scope, node, nd, "file")? {
            Some(file) if !file.text.is_empty() => file,
            _ => {
                return Err(Error::BadValue { at: at.clone(), message: "hextiledimage has no file".to_string() });
            }
        };
        let colorspace = file.colorspace.clone();
        let texture = self.dag.add_texture(TextureDesc {
            file: file.text,
            colorspace: colorspace.clone(),
            address_u: AddressMode::Periodic,
            address_v: AddressMode::Periodic,
            filter: FilterMode::Linear,
        });
        let default = self.constant_input(scope, node, nd, "default", at)?;
        let texcoord = self.input_value(scope, node, nd, "texcoord")?;
        let tiling = self.input_value(scope, node, nd, "tiling")?;
        let mut params = Vec::new();
        for name in TILE_INPUTS {
            params.push(self.input_value(scope, node, nd, name)?);
        }
        let lumacoeffs = self.input_value(scope, node, nd, "lumacoeffs")?;
        let contrast = self.input_value(scope, node, nd, "falloffcontrast")?;
        let falloff = self.input_value(scope, node, nd, "falloff")?;

        let origin = at.path.clone();
        let mut add = |kind: NodeKind, ty: Type| self.dag.add(IrNode { kind, ty, origin: origin.clone() });
        let op = |op: Op, args: Vec<NodeId>| NodeKind::Op { op, args };
        // mx_transform_uv(texcoord, tiling, 0); the V flip lives in the hextile ops.
        let coord = add(op(Op::Multiply, vec![texcoord, tiling]), Type::Vector2);
        let dx = add(op(Op::Dpdx, vec![coord]), Type::Vector2);
        let dy = add(op(Op::Dpdy, vec![coord]), Type::Vector2);
        let mut samples = Vec::new();
        let mut luminances = Vec::new();
        for tile in 0..3u8 {
            let uv = add(op(Op::HextileCoord(tile), [&[coord][..], &params].concat()), Type::Vector2);
            let gx = add(op(Op::HextileGrad(tile), [&[dx, coord][..], &params[..4]].concat()), Type::Vector2);
            let gy = add(op(Op::HextileGrad(tile), [&[dy, coord][..], &params[..4]].concat()), Type::Vector2);
            let sample = add(NodeKind::Sample { texture, uv, default: default.clone(), grad: Some([gx, gy]) }, ty);
            // dot(c.rgb, lumacoeffs)
            let luminance = add(op(Op::Luminance, vec![sample, lumacoeffs]), ty);
            luminances.push(add(op(Op::Extract(0), vec![luminance]), Type::Float));
            samples.push(sample);
        }
        // mix(vec3(1.0), cw, falloff_contrast)
        let cw = add(op(Op::Combine, luminances), Type::Vector3);
        let one = add(NodeKind::Const(Value::Vector3([1.0; 3])), Type::Vector3);
        let cw = add(op(Op::Mix, vec![cw, one, contrast]), Type::Vector3);
        let w = add(op(Op::HextileWeights, vec![coord, cw, falloff]), Type::Vector3);
        // Color4 blends alpha by the tile weights alone: luminance weights of one.
        let alpha_w =
            (ty == Type::Color4).then(|| add(op(Op::HextileWeights, vec![coord, one, falloff]), Type::Vector3));
        let mut terms = Vec::new();
        for (i, sample) in samples.into_iter().enumerate() {
            let wi = add(op(Op::Extract(i as u32), vec![w]), Type::Float);
            let weight = match alpha_w {
                Some(aw) => {
                    let ai = add(op(Op::Extract(i as u32), vec![aw]), Type::Float);
                    add(op(Op::Combine, vec![wi, wi, wi, ai]), Type::Color4)
                }
                None => wi,
            };
            terms.push(add(op(Op::Multiply, vec![sample, weight]), ty));
        }
        let partial = add(op(Op::Add, vec![terms[0], terms[1]]), ty);
        let blend = add(op(Op::Add, vec![partial, terms[2]]), ty);
        self.convert_sample_colorspace(blend, colorspace.as_deref(), at)
    }
}

#[cfg(test)]
mod tests {
    use shadewire::interpret::{ContextSource, TextureSource, eval_surface};
    use shadewire::{ContextRead, NodeKind, Op, Space, TextureDesc, TransformKind, Value};

    use super::super::elaborate_output;
    use super::super::tests::doc;

    struct Uv;
    impl ContextSource for Uv {
        fn read(&mut self, read: &ContextRead) -> Value {
            assert!(matches!(read, ContextRead::TexCoord { index: 0, .. }), "{read:?}");
            Value::Vector2([0.3, 0.8])
        }
        fn transform(&mut self, _: TransformKind, _: Space, _: Space, v: [f32; 3]) -> [f32; 3] {
            v
        }
    }

    /// Every texel is the same color: the blend of three samples must give it back.
    struct Flat([f32; 4]);
    impl TextureSource for Flat {
        fn sample(&self, _: &TextureDesc, _: [f32; 2]) -> Option<[f32; 4]> {
            Some(self.0)
        }
    }

    fn graph(ty: &str) -> String {
        format!(
            r#"<nodegraph name="g"><hextiledimage name="h" type="{ty}">
                 <input name="file" type="filename" value="onyx.png"/><input name="falloff" type="float" value="0.75"/>
               </hextiledimage><output name="out" type="{ty}" nodename="h"/></nodegraph>"#
        )
    }

    #[test]
    fn three_gradient_samples_blend_back_a_flat_image() {
        let dag = elaborate_output(&doc(&graph("color3")), "g", "out").unwrap();
        let samples: Vec<_> =
            dag.nodes.iter().filter(|n| matches!(n.kind, NodeKind::Sample { grad: Some(_), .. })).collect();
        assert_eq!(samples.len(), 3);
        assert!(dag.nodes.iter().any(|n| matches!(n.kind, NodeKind::Op { op: Op::Dpdy, .. })));
        let out = eval_surface(&dag, &mut Uv, &Flat([0.2, 0.4, 0.6, 1.0])).remove(0).1.floats();
        for (o, t) in out.iter().zip([0.2, 0.4, 0.6]) {
            assert!((o - t).abs() < 1e-6, "{out:?}");
        }
    }

    #[test]
    fn color4_blends_alpha_too() {
        let dag = elaborate_output(&doc(&graph("color4")), "g", "out").unwrap();
        let out = eval_surface(&dag, &mut Uv, &Flat([0.2, 0.4, 0.6, 0.5])).remove(0).1.floats();
        for (o, t) in out.iter().zip([0.2, 0.4, 0.6, 0.5]) {
            assert!((o - t).abs() < 1e-6, "{out:?}");
        }
    }

    #[test]
    fn no_file_is_a_bad_value() {
        let text = graph("color3").replace(r#"<input name="file" type="filename" value="onyx.png"/>"#, "");
        let err = elaborate_output(&doc(&text), "g", "out").unwrap_err();
        assert!(err.to_string().contains("hextiledimage has no file"), "{err}");
    }
}
