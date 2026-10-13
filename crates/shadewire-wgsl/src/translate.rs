//! Translates a linked WGSL module to the other shading languages through naga.

use naga::back::{glsl, hlsl, msl, spv, wgsl};
use shadewire::Error;

use crate::link::parse_and_validate;
use crate::target::{GlslVersion, HlslShaderModel, Target};

#[derive(Clone, Debug, PartialEq)]
pub enum Output {
    Text(String),
    Words(Vec<u32>),
}

/// The OpenGL binding point of WGSL `@group(g) @binding(b)`: `32 * g + b`, so the groups never collide. GLSL
/// 4.20 and ES 3.10 write it as `layout(binding = ...)`; a texture and its sampler combine into one `sampler2D`
/// at the texture's binding point.
pub fn glsl_binding(group: u32, binding: u32) -> Option<u8> {
    u8::try_from(32 * group + binding).ok().filter(|_| binding < 32)
}

/// What naga writes for `ADJUST_COORDINATE_SPACE` (Y negated for wgpu's GL backend, depth to -1..1), and the
/// depth remap alone, which is what drawing with OpenGL directly needs.
const GLSL_FLIP: &str = "gl_Position.yz = vec2(-gl_Position.y, gl_Position.z * 2.0 - gl_Position.w);";
const GLSL_DEPTH_ONLY: &str = "gl_Position.z = gl_Position.z * 2.0 - gl_Position.w;";

/// A linked module in `target`. `entry_point` names the entry point to write (GLSL, HLSL and SPIR-V write one);
/// None takes the first fragment entry point, else the first. GLSL follows OpenGL's conventions: clip-space
/// depth from -1 to 1 (WGSL's is 0 to 1), and resources at `glsl_binding` points.
pub fn translate(linked: &str, target: Target, entry_point: Option<&str>) -> Result<Output, Error> {
    let (module, info) = parse_and_validate(linked)?;
    let chosen = match entry_point {
        Some(name) => Some(
            module
                .entry_points
                .iter()
                .find(|e| e.name == name)
                .ok_or_else(|| Error::Naga(format!("no entry point {name}")))?,
        ),
        None => module
            .entry_points
            .iter()
            .find(|e| e.stage == naga::ShaderStage::Fragment)
            .or_else(|| module.entry_points.first()),
    };
    let entry = chosen.map(|e| (e.stage, e.name.clone()));
    match target {
        Target::Wgsl => wgsl::write_string(&module, &info, wgsl::WriterFlags::empty())
            .map(Output::Text)
            .map_err(|e| Error::Naga(e.to_string())),
        Target::Hlsl(model) => {
            // Register and space follow the WGSL group and binding. HLSL spaces are u8 here.
            let mut binding_map = hlsl::BindingMap::default();
            for (_, global) in module.global_variables.iter() {
                if let Some(binding) = &global.binding {
                    let space = u8::try_from(binding.group)
                        .map_err(|_| Error::Naga(format!("bind group too large for HLSL space: {}", binding.group)))?;
                    binding_map
                        .insert(*binding, hlsl::BindTarget { space, register: binding.binding, ..Default::default() });
                }
            }
            let options = hlsl::Options { shader_model: hlsl_model(model), binding_map, ..Default::default() };
            let pipeline = hlsl::PipelineOptions { entry_point: entry };
            let mut out = String::new();
            hlsl::Writer::new(&mut out, &options, &pipeline)
                .write(&module, &info, None)
                .map_err(|e| Error::Naga(e.to_string()))?;
            Ok(Output::Text(out))
        }
        Target::Glsl(version) => {
            let (stage, name) = entry.ok_or_else(|| Error::Naga("GLSL needs an entry point".into()))?;
            let mut binding_map = glsl::BindingMap::default();
            for (_, global) in module.global_variables.iter() {
                if let Some(binding) = &global.binding {
                    let point = glsl_binding(binding.group, binding.binding).ok_or_else(|| {
                        Error::Naga(format!(
                            "group {} binding {} has no GLSL binding point",
                            binding.group, binding.binding
                        ))
                    })?;
                    binding_map.insert(*binding, point);
                }
            }
            let glsl_options = glsl::Options { version: glsl_version(version), binding_map, ..Default::default() };
            let pipeline = glsl::PipelineOptions { shader_stage: stage, entry_point: name, multiview: None };
            let mut out = String::new();
            glsl::Writer::new(
                &mut out,
                &module,
                &info,
                &glsl_options,
                &pipeline,
                naga::proc::BoundsCheckPolicies::default(),
            )
            .and_then(|mut w| w.write().map(|_| ()))
            .map_err(|e| Error::Naga(e.to_string()))?;
            Ok(Output::Text(out.replace(GLSL_FLIP, GLSL_DEPTH_ONLY)))
        }
        Target::Msl(version) => {
            // Phase 1: resource slots follow declaration order.
            let options = msl::Options {
                lang_version: (version.major, version.minor),
                fake_missing_bindings: true,
                ..Default::default()
            };
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

fn hlsl_model(model: HlslShaderModel) -> hlsl::ShaderModel {
    match model {
        HlslShaderModel::Sm5_0 => hlsl::ShaderModel::V5_0,
        HlslShaderModel::Sm5_1 => hlsl::ShaderModel::V5_1,
        HlslShaderModel::Sm6_0 => hlsl::ShaderModel::V6_0,
        HlslShaderModel::Sm6_1 => hlsl::ShaderModel::V6_1,
        HlslShaderModel::Sm6_2 => hlsl::ShaderModel::V6_2,
        HlslShaderModel::Sm6_3 => hlsl::ShaderModel::V6_3,
        HlslShaderModel::Sm6_4 => hlsl::ShaderModel::V6_4,
        HlslShaderModel::Sm6_5 => hlsl::ShaderModel::V6_5,
        HlslShaderModel::Sm6_6 => hlsl::ShaderModel::V6_6,
        HlslShaderModel::Sm6_7 => hlsl::ShaderModel::V6_7,
        HlslShaderModel::Sm6_8 => hlsl::ShaderModel::V6_8,
        HlslShaderModel::Sm6_9 => hlsl::ShaderModel::V6_9,
    }
}

fn glsl_version(version: GlslVersion) -> glsl::Version {
    match version {
        GlslVersion::Desktop(v) => glsl::Version::Desktop(v),
        GlslVersion::Es(v) => glsl::Version::Embedded { version: v, is_webgl: false },
        GlslVersion::WebGl2 => glsl::Version::Embedded { version: 300, is_webgl: true },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::test_support::add;
    use crate::target::MslVersion;
    use crate::{EmitOptions, emit, link::link, reference_host};
    use shadewire::*;

    fn linked() -> String {
        linked_at(2)
    }

    fn linked_at(group: u32) -> String {
        let mut dag = Dag::new("open_pbr_surface");
        let tint = add(
            &mut dag,
            NodeKind::Uniform { name: "tint".into(), value: Value::Color3([1.0, 0.5, 0.25]) },
            Type::Color3,
        );
        let uv = add(&mut dag, NodeKind::Context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }), Type::Vector2);
        let t = dag.add_texture(TextureDesc {
            file: "a.png".into(),
            colorspace: None,
            address_u: AddressMode::Periodic,
            address_v: AddressMode::Periodic,
            filter: FilterMode::Linear,
        });
        let tex = add(
            &mut dag,
            NodeKind::Sample { texture: t, uv, default: Value::Color3([0.0; 3]), grad: None },
            Type::Color3,
        );
        let amp = add(&mut dag, NodeKind::Const(Value::Float(1.0)), Type::Float);
        let pivot = add(&mut dag, NodeKind::Const(Value::Float(0.0)), Type::Float);
        let noise = add(&mut dag, NodeKind::Op { op: Op::Noise2d, args: vec![amp, pivot, uv] }, Type::Float);
        let mul = add(&mut dag, NodeKind::Op { op: Op::Mix, args: vec![tint, tex, noise] }, Type::Color3);
        dag.surface.fields.push(SurfaceField { name: "base_color".into(), ty: Type::Color3, node: mul });
        dag.surface.fields.push(SurfaceField { name: "base_weight".into(), ty: Type::Float, node: noise });
        let info = SourceInfo { source: "t".into(), mtlx_version: "1.39".into() };
        let r =
            reflect(&dag, &BindingOptions { group, first_binding: 0, reserve_uniform_binding: false }, &info).unwrap();
        let generated = emit(&dag, &r, &EmitOptions::default());
        let prelude = reference_host::prelude(&r, "OpenPbrSurface");
        link(&generated, &prelude).unwrap_or_else(|e| panic!("{generated}\n{e}"))
    }

    fn text(target: Target) -> String {
        match translate(&linked(), target, None).unwrap() {
            Output::Text(t) => t,
            Output::Words(_) => panic!("expected text"),
        }
    }

    #[test]
    fn glsl_420_places_each_group_at_its_own_binding_points() {
        let Output::Text(text) = translate(&linked_at(2), Target::Glsl(GlslVersion::Desktop(450)), None).unwrap()
        else {
            panic!("text")
        };
        // Group 2: the uniform block at binding 0, the texture (combined with its sampler) at binding 1.
        assert!(text.contains("binding = 64"), "{text}");
        assert!(
            text.contains("binding = 65) uniform highp sampler2D") || text.contains("binding = 65) uniform sampler2D"),
            "{text}"
        );
        assert_eq!(glsl_binding(1, 3), Some(35));
        assert_eq!(glsl_binding(0, 40), None);
    }

    #[test]
    fn an_entry_point_is_picked_by_name_and_glsl_keeps_opengl_conventions() {
        let linked = format!(
            "{}
@vertex fn vs_main(@location(0) p: vec3<f32>) -> @builtin(position) vec4<f32> {{ return vec4<f32>(p, 1.0); }}
",
            linked()
        );
        let Output::Text(glsl) = translate(&linked, Target::Glsl(GlslVersion::Desktop(420)), Some("vs_main")).unwrap()
        else {
            panic!("text")
        };
        assert!(glsl.contains(GLSL_DEPTH_ONLY) && !glsl.contains(GLSL_FLIP), "{glsl}");
        assert!(translate(&linked, Target::Glsl(GlslVersion::Desktop(420)), Some("nope")).is_err());
    }

    #[test]
    fn every_target_translates() {
        let wgsl = text(Target::Wgsl);
        assert!(wgsl.contains("shadewire_eval_surface"));
        let hlsl = text(Target::Hlsl(HlslShaderModel::Sm6_0));
        assert!(hlsl.contains("register(b0, space2)"), "{hlsl}");
        assert!(hlsl.contains("register(t1, space2)"), "{hlsl}");
        assert!(hlsl.contains("register(s0, space0)"), "{hlsl}");
        let glsl = text(Target::Glsl(GlslVersion::Es(300)));
        assert!(glsl.contains("#version 300 es"), "{glsl}");
        let desktop = text(Target::Glsl(GlslVersion::Desktop(450)));
        assert!(desktop.contains("#version 450"), "{desktop}");
        let webgl = text(Target::Glsl(GlslVersion::WebGl2));
        assert!(webgl.contains("#version 300 es"), "{webgl}");
        assert!(!text(Target::Msl(MslVersion { major: 2, minor: 1 })).is_empty());
        match translate(&linked(), Target::SpirV, None).unwrap() {
            Output::Words(w) => assert_eq!(w[0], 0x0723_0203),
            Output::Text(_) => panic!("expected words"),
        }
    }

    #[test]
    fn a_group_past_255_has_no_hlsl_space() {
        let hlsl = Target::Hlsl(HlslShaderModel::Sm6_0);
        assert!(translate(&linked_at(255), hlsl, None).is_ok());
        match translate(&linked_at(256), hlsl, None) {
            Err(Error::Naga(message)) => assert_eq!(message, "bind group too large for HLSL space: 256"),
            other => panic!("expected a naga error, got {other:?}"),
        }
    }

    #[test]
    fn bad_wgsl_is_a_naga_error() {
        assert!(matches!(translate("fn (", Target::Wgsl, None), Err(Error::Naga(_))));
        assert!(matches!(link("fn (", ""), Err(Error::Naga(_))));
    }
}
