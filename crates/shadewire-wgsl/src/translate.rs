//! Translates a linked WGSL module to the other shading languages through naga.

use naga::back::{glsl, hlsl, msl, spv, wgsl};
use shadewire_ir::Error;

use crate::link::parse_and_validate;

#[derive(Clone, Debug)]
pub enum Target {
    Wgsl,
    Hlsl { shader_model: naga::back::hlsl::ShaderModel },
    Glsl { version: naga::back::glsl::Version },
    Msl { version: (u8, u8) },
    SpirV,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Output {
    Text(String),
    Words(Vec<u32>),
}

pub fn translate(linked: &str, target: Target) -> Result<Output, Error> {
    let (module, info) = parse_and_validate(linked)?;
    // The first fragment entry point if there is one, else the first entry point.
    let chosen = module
        .entry_points
        .iter()
        .find(|e| e.stage == naga::ShaderStage::Fragment)
        .or_else(|| module.entry_points.first());
    let entry = chosen.map(|e| (e.stage, e.name.clone()));
    match target {
        Target::Wgsl => wgsl::write_string(&module, &info, wgsl::WriterFlags::empty())
            .map(Output::Text)
            .map_err(|e| Error::Naga(e.to_string())),
        Target::Hlsl { shader_model } => {
            // Register and space follow the WGSL group and binding.
            let mut binding_map = hlsl::BindingMap::default();
            for (_, global) in module.global_variables.iter() {
                if let Some(binding) = &global.binding {
                    binding_map.insert(
                        binding.clone(),
                        hlsl::BindTarget { space: binding.group as u8, register: binding.binding, ..Default::default() },
                    );
                }
            }
            let options = hlsl::Options { shader_model, binding_map, ..Default::default() };
            let pipeline = hlsl::PipelineOptions { entry_point: entry };
            let mut out = String::new();
            hlsl::Writer::new(&mut out, &options, &pipeline)
                .write(&module, &info, None)
                .map_err(|e| Error::Naga(e.to_string()))?;
            Ok(Output::Text(out))
        }
        Target::Glsl { version } => {
            let (stage, name) = entry.ok_or_else(|| Error::Naga("GLSL needs an entry point".into()))?;
            let options = glsl::Options { version, ..Default::default() };
            let pipeline = glsl::PipelineOptions { shader_stage: stage, entry_point: name, multiview: None };
            let mut out = String::new();
            glsl::Writer::new(&mut out, &module, &info, &options, &pipeline, naga::proc::BoundsCheckPolicies::default())
                .and_then(|mut w| w.write().map(|_| ()))
                .map_err(|e| Error::Naga(e.to_string()))?;
            Ok(Output::Text(out))
        }
        Target::Msl { version } => {
            // Phase 1: resource slots follow declaration order.
            let options = msl::Options { lang_version: version, fake_missing_bindings: true, ..Default::default() };
            msl::write_string(&module, &info, &options, &msl::PipelineOptions::default())
                .map(|(text, _)| Output::Text(text))
                .map_err(|e| Error::Naga(e.to_string()))
        }
        Target::SpirV => {
            let pipeline = entry.map(|(stage, name)| spv::PipelineOptions { shader_stage: stage, entry_point: name });
            spv::write_vec(&module, &info, &spv::Options::default(), pipeline.as_ref())
                .map(Output::Words)
                .map_err(|e| Error::Naga(e.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::test_support::add;
    use crate::{emit, link::link, reference_host, EmitOptions};
    use naga::back::{glsl, hlsl};
    use shadewire_ir::*;

    fn linked() -> String {
        let mut dag = Dag::new("open_pbr_surface");
        let tint = add(&mut dag, NodeKind::Uniform { name: "tint".into(), value: Value::Color3([1.0, 0.5, 0.25]) }, Type::Color3);
        let uv = add(&mut dag, NodeKind::Context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }), Type::Vector2);
        let t = dag.add_texture(TextureDesc {
            file: "a.png".into(),
            colorspace: None,
            address_u: AddressMode::Periodic,
            address_v: AddressMode::Periodic,
            filter: FilterMode::Linear,
        });
        let tex = add(&mut dag, NodeKind::Sample { texture: t, uv, default: Value::Color3([0.0; 3]) }, Type::Color3);
        let amp = add(&mut dag, NodeKind::Const(Value::Float(1.0)), Type::Float);
        let pivot = add(&mut dag, NodeKind::Const(Value::Float(0.0)), Type::Float);
        let noise = add(&mut dag, NodeKind::Op { op: Op::Noise2d, args: vec![amp, pivot, uv] }, Type::Float);
        let mul = add(&mut dag, NodeKind::Op { op: Op::Mix, args: vec![tint, tex, noise] }, Type::Color3);
        dag.surface.fields.push(SurfaceField { name: "base_color".into(), ty: Type::Color3, node: mul });
        dag.surface.fields.push(SurfaceField { name: "base_weight".into(), ty: Type::Float, node: noise });
        let info = SourceInfo { source: "t".into(), mtlx_version: "1.39".into() };
        let r = reflect(&dag, &BindingOptions { group: 2, first_binding: 0 }, &info);
        let generated = emit(&dag, &r, &EmitOptions::default());
        let prelude = reference_host::prelude(&r, "OpenPbrSurface");
        link(&generated, &prelude).unwrap_or_else(|e| panic!("{generated}\n{e}"))
    }

    fn text(target: Target) -> String {
        match translate(&linked(), target).unwrap() {
            Output::Text(t) => t,
            Output::Words(_) => panic!("expected text"),
        }
    }

    #[test]
    fn every_target_translates() {
        let wgsl = text(Target::Wgsl);
        assert!(wgsl.contains("sw_eval_surface"));
        let hlsl = text(Target::Hlsl { shader_model: hlsl::ShaderModel::V6_0 });
        assert!(hlsl.contains("register(b0, space2)"), "{hlsl}");
        assert!(hlsl.contains("register(t1, space2)"), "{hlsl}");
        assert!(hlsl.contains("register(s0, space0)"), "{hlsl}");
        let glsl = text(Target::Glsl { version: glsl::Version::Embedded { version: 300, is_webgl: false } });
        assert!(glsl.contains("#version 300 es"), "{glsl}");
        assert!(!text(Target::Msl { version: (2, 1) }).is_empty());
        match translate(&linked(), Target::SpirV).unwrap() {
            Output::Words(w) => assert_eq!(w[0], 0x0723_0203),
            Output::Text(_) => panic!("expected words"),
        }
    }

    #[test]
    fn bad_wgsl_is_a_naga_error() {
        assert!(matches!(translate("fn (", Target::Wgsl), Err(Error::Naga(_))));
        assert!(matches!(link("fn (", ""), Err(Error::Naga(_))));
    }
}
