//! The WGSL back end's compile: the shared steps, printed as WGSL.

use shadewire::shader::material::{MaterialOptions, material};
use shadewire::shader::{CompileOptions, Compiled, compile_with};
use shadewire::{Dag, Error, Reflection, SourceInfo};

/// The material's code as WGSL: its shader tree, printed.
pub fn emit(dag: &Dag, reflection: &Reflection, options: &MaterialOptions) -> String {
    crate::print(&material(dag, reflection, options))
}

/// Optimize (when asked), reflect, print as WGSL.
pub fn compile(dag: &Dag, source: &SourceInfo, options: &CompileOptions) -> Result<Compiled, Error> {
    compile_with(dag, source, options, |module, _| crate::print(module))
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
        assert!(out.code.contains("@binding(1)"), "{}", out.code);
    }

    #[test]
    fn unreserved_uniform_binding_keeps_the_first_texture_at_first_binding() {
        let out = compile(&texture_only_dag(), &info(), &CompileOptions::default()).unwrap();
        assert_eq!(out.reflection.textures[0].binding, 0);
    }
}
