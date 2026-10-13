//! The back end: any front end's IR to WGSL and reflection.

use shadewire::optimize::optimize;
use shadewire::{BindingOptions, Dag, Error, Reflection, SourceInfo, reflect};

use crate::compile_options::CompileOptions;
use crate::emit::{EmitOptions, emit};

/// The result of a compilation: the generated module, what a host must bind, and the DAG
/// the module was emitted from (optimized when `CompileOptions::optimize`).
#[derive(Clone, Debug)]
pub struct Compiled {
    pub wgsl: String,
    pub reflection: Reflection,
    pub dag: Dag,
}

/// Optimize (when asked), reflect, emit.
pub fn compile(dag: &Dag, source: &SourceInfo, options: &CompileOptions) -> Result<Compiled, Error> {
    let dag = if options.optimize { optimize(dag) } else { dag.clone() };
    let bindings = BindingOptions {
        group: options.group,
        first_binding: options.first_binding,
        reserve_uniform_binding: options.reserve_uniform_binding,
    };
    let reflection = reflect(&dag, &bindings, source)?;
    let emit_options = EmitOptions { flip_v: options.flip_v, implicit_derivatives: options.implicit_derivatives };
    let wgsl = emit(&dag, &reflection, &emit_options);
    Ok(Compiled { wgsl, reflection, dag })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shadewire::{AddressMode, FilterMode, Node, NodeKind, SurfaceField, TextureDesc, Type, Value};

    fn texture_only_dag() -> Dag {
        let mut dag = Dag::new("open_pbr_surface");
        let texture = dag.add_texture(TextureDesc {
            file: "a.png".into(),
            colorspace: None,
            address_u: AddressMode::Periodic,
            address_v: AddressMode::Periodic,
            filter: FilterMode::Linear,
        });
        let uv =
            dag.add(Node { kind: NodeKind::Const(Value::Vector2([0.5; 2])), ty: Type::Vector2, origin: String::new() });
        let kind = NodeKind::Sample { texture, uv, default: Value::Float(0.0), grad: None };
        let sample = dag.add(Node { kind, ty: Type::Float, origin: String::new() });
        dag.surface.fields.push(SurfaceField { name: "base_weight".into(), ty: Type::Float, node: sample });
        dag
    }

    fn info() -> SourceInfo {
        SourceInfo { source: "a.mtlx".into(), mtlx_version: "1.39".into() }
    }

    #[test]
    fn reserved_uniform_binding_moves_the_first_texture() {
        let reserved = CompileOptions { reserve_uniform_binding: true, ..CompileOptions::default() };
        let out = compile(&texture_only_dag(), &info(), &reserved).unwrap();
        assert_eq!(out.reflection.uniform_binding, None);
        assert_eq!(out.reflection.textures[0].binding, 1);
        assert!(out.wgsl.contains("@binding(1)"), "{}", out.wgsl);
    }

    #[test]
    fn unreserved_uniform_binding_keeps_the_first_texture_at_first_binding() {
        let out = compile(&texture_only_dag(), &info(), &CompileOptions::default()).unwrap();
        assert_eq!(out.reflection.textures[0].binding, 0);
    }
}
