//! The `image` node and `tiledimage`, which reaches it through its interface.

use shadewire_ir::{AddressMode, ContextRead, Dag, Error, FilterMode, NodeId, NodeKind, Op, TextureDesc, Type, Value};

use super::elaborate_output;
use super::tests::doc;

/// The sampled texture, uv node and default of the DAG's only sample (color
/// samples sit under their colorspace conversion).
fn sample(dag: &Dag) -> (&TextureDesc, NodeId, Value) {
    let mut samples = dag.nodes.iter().filter_map(|n| match &n.kind {
        NodeKind::Sample { texture, uv, default, .. } => Some((&dag.textures[*texture], *uv, default.clone())),
        _ => None,
    });
    let found = samples.next().expect("no sample");
    assert!(samples.next().is_none(), "more than one sample");
    found
}

#[test]
fn image_samples_its_file_with_modes_and_default() {
    let d = doc(r#"<nodegraph name="g" fileprefix="tex/">
          <image name="i" type="color3">
            <input name="file" type="filename" value="wood.png" colorspace="srgb_texture"/>
            <input name="default" type="color3" value="1,0,1"/>
            <input name="vaddressmode" type="string" value="clamp"/>
            <input name="filtertype" type="string" value="closest"/>
          </image>
          <output name="out" type="color3" nodename="i"/>
        </nodegraph>"#);
    let dag = elaborate_output(&d, "g", "out").unwrap();
    let (texture, uv, default) = sample(&dag);
    assert_eq!(
        *texture,
        TextureDesc {
            file: "tex/wood.png".to_string(),
            colorspace: Some("srgb_texture".to_string()),
            address_u: AddressMode::Periodic,
            address_v: AddressMode::Clamp,
            filter: FilterMode::Closest,
        }
    );
    assert_eq!(default, Value::Color3([1.0, 0.0, 1.0]));
    assert_eq!(dag.node(uv).kind, NodeKind::Context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }));
    assert_eq!(dag.node(dag.surface.fields[0].node).ty, Type::Color3);
}

#[test]
fn image_without_a_file_is_a_bad_value() {
    let d = doc(r#"<nodegraph name="g">
          <image name="i" type="float"/>
          <output name="out" type="float" nodename="i"/>
        </nodegraph>"#);
    let err = elaborate_output(&d, "g", "out").unwrap_err();
    assert!(matches!(err, Error::BadValue { .. }), "{err:?}");
}

#[test]
fn unknown_address_mode_is_a_bad_value() {
    let d = doc(r#"<nodegraph name="g">
          <image name="i" type="float">
            <input name="file" type="filename" value="a.png"/>
            <input name="uaddressmode" type="string" value="wrap"/>
          </image>
          <output name="out" type="float" nodename="i"/>
        </nodegraph>"#);
    let err = elaborate_output(&d, "g", "out").unwrap_err();
    assert!(matches!(err, Error::BadValue { .. }), "{err:?}");
}

#[test]
fn two_images_of_one_file_share_a_texture() {
    let d = doc(r#"<nodegraph name="g">
          <image name="a" type="float"><input name="file" type="filename" value="a.png"/></image>
          <image name="b" type="float"><input name="file" type="filename" value="a.png"/></image>
          <add name="s" type="float">
            <input name="in1" type="float" nodename="a"/>
            <input name="in2" type="float" nodename="b"/>
          </add>
          <output name="out" type="float" nodename="s"/>
        </nodegraph>"#);
    let dag = elaborate_output(&d, "g", "out").unwrap();
    assert_eq!(dag.textures.len(), 1);
}

#[test]
fn uvtiling_and_uvoffset_scale_then_offset_the_texcoord() {
    // mx_transform_uv: uv * uv_scale + uv_offset.
    let d = doc(r#"<nodedef name="ND_image_tiled" node="image">
          <input name="file" type="filename" value=""/>
          <input name="default" type="float" value="0"/>
          <input name="texcoord" type="vector2" defaultgeomprop="UV0"/>
          <input name="uvtiling" type="vector2" value="1,1"/>
          <input name="uvoffset" type="vector2" value="0,0"/>
          <output name="out" type="float"/>
        </nodedef>
        <nodegraph name="g">
          <image name="i" type="float" nodedef="ND_image_tiled">
            <input name="file" type="filename" value="a.png"/>
            <input name="uvtiling" type="vector2" value="2,3"/>
          </image>
          <output name="out" type="float" nodename="i"/>
        </nodegraph>"#);
    let dag = elaborate_output(&d, "g", "out").unwrap();
    let (_, uv, _) = sample(&dag);
    let NodeKind::Op { op: Op::Add, args } = &dag.node(uv).kind else { panic!("uv is not an add: {:?}", dag.node(uv)) };
    assert_eq!(dag.node(args[1]).kind, NodeKind::Const(Value::Vector2([0.0, 0.0])));
    let NodeKind::Op { op: Op::Multiply, args } = &dag.node(args[0]).kind else { panic!("no multiply") };
    assert_eq!(dag.node(args[0]).kind, NodeKind::Context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }));
    assert_eq!(dag.node(args[1]).kind, NodeKind::Const(Value::Vector2([2.0, 3.0])));
}

#[test]
fn tiledimage_keeps_the_authored_file_colorspace() {
    let d = doc(r#"<nodegraph name="g">
          <tiledimage name="t" type="color3">
            <input name="file" type="filename" value="brick.png" colorspace="srgb_texture"/>
          </tiledimage>
          <output name="out" type="color3" nodename="t"/>
        </nodegraph>"#);
    let dag = elaborate_output(&d, "g", "out").unwrap();
    let (texture, _, _) = sample(&dag);
    assert_eq!(texture.file, "brick.png");
    assert_eq!(texture.colorspace.as_deref(), Some("srgb_texture"));
}
