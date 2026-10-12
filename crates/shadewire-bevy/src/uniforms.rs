//! Packs a reflection's uniform block into the extension's fixed `[Vec4; 64]` binding.

use bevy::math::Vec4;
use shadewire::{Reflection, Value};

use crate::error::ShadewireError;

/// Bytes in the extension's uniform binding (`[Vec4; 64]`).
pub const UNIFORM_BYTES: u32 = 1024;

/// Every reflection uniform written at its byte offset: f32 as is, i32 and booleans (u32) as bits.
pub fn pack(reflection: &Reflection) -> Result<[Vec4; 64], ShadewireError> {
    check_size(reflection)?;
    let mut block = [Vec4::ZERO; 64];
    for uniform in &reflection.uniforms {
        put(&mut block, uniform.offset, &uniform.default);
    }
    Ok(block)
}

/// Writes `value` over the uniform `name` of `reflection` in `block`, a block packed from that reflection.
pub fn write(block: &mut [Vec4; 64], reflection: &Reflection, name: &str, value: &Value) -> Result<(), ShadewireError> {
    check_size(reflection)?;
    let uniform = reflection
        .uniforms
        .iter()
        .find(|u| u.name == name)
        .ok_or_else(|| ShadewireError::UnknownUniform(name.to_string()))?;
    if value.ty() != uniform.ty {
        return Err(ShadewireError::UniformType {
            name: name.to_string(),
            expected: format!("{:?}", uniform.ty),
            found: format!("{:?}", value.ty()),
        });
    }
    put(block, uniform.offset, value);
    Ok(())
}

fn check_size(reflection: &Reflection) -> Result<(), ShadewireError> {
    if reflection.uniform_block_size > UNIFORM_BYTES {
        return Err(ShadewireError::UniformsTooLarge { size: reflection.uniform_block_size, limit: UNIFORM_BYTES });
    }
    Ok(())
}

/// `value`'s words from byte `offset` on.
fn put(block: &mut [Vec4; 64], offset: u32, value: &Value) {
    let first = (offset / 4) as usize;
    for (i, bits) in value_words(value).into_iter().enumerate() {
        let word = first + i;
        block[word / 4][word % 4] = f32::from_bits(bits);
    }
}

/// The 32-bit words a value occupies in a WGSL uniform block, padding included (mat3x3 columns are 16 bytes).
fn value_words(value: &Value) -> Vec<u32> {
    match value {
        Value::Boolean(b) => vec![u32::from(*b)],
        Value::Integer(i) => vec![*i as u32],
        Value::Matrix33(m) => {
            let mut out = Vec::with_capacity(12);
            for column in m.chunks(3) {
                out.extend(column.iter().map(|x| x.to_bits()));
                out.push(0);
            }
            out
        }
        other => other.floats().iter().map(|x| x.to_bits()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shadewire::{BindingOptions, Dag, Node, NodeKind, SourceInfo, SurfaceField, Value, reflect};

    fn reflection_with(uniforms: &[(&str, Value)]) -> Reflection {
        let mut dag = Dag::new("m");
        for (name, value) in uniforms {
            let ty = value.ty();
            let kind = NodeKind::Uniform { name: name.to_string(), value: value.clone() };
            let id = dag.add(Node { kind, ty, origin: String::new() });
            dag.surface.fields.push(SurfaceField { name: name.to_string(), ty, node: id });
        }
        let info = SourceInfo { source: "t".into(), mtlx_version: "1.39".into() };
        let options = BindingOptions { group: 3, first_binding: 100, reserve_uniform_binding: true };
        reflect(&dag, &options, &info).unwrap()
    }

    fn word(block: &[Vec4; 64], offset: usize) -> u32 {
        block[offset / 16][(offset % 16) / 4].to_bits()
    }

    #[test]
    fn float_after_vec3_lands_at_12() {
        let r = reflection_with(&[("tint", Value::Color3([0.5, 0.25, 0.125])), ("coat", Value::Float(0.75))]);
        let block = pack(&r).unwrap();
        assert_eq!(block[0], Vec4::new(0.5, 0.25, 0.125, 0.75));
    }

    #[test]
    fn booleans_and_integers_are_bits() {
        let r = reflection_with(&[("flag", Value::Boolean(true)), ("n", Value::Integer(-3))]);
        let offsets: Vec<u32> = r.uniforms.iter().map(|u| u.offset).collect();
        let block = pack(&r).unwrap();
        assert_eq!(word(&block, offsets[0] as usize), 1);
        assert_eq!(word(&block, offsets[1] as usize), (-3i32) as u32);
    }

    #[test]
    fn write_replaces_one_uniform_by_name() {
        let r = reflection_with(&[("tint", Value::Color3([0.5, 0.25, 0.125])), ("coat", Value::Float(0.75))]);
        let mut block = pack(&r).unwrap();
        write(&mut block, &r, "coat", &Value::Float(0.5)).unwrap();
        write(&mut block, &r, "tint", &Value::Color3([1.0, 2.0, 3.0])).unwrap();
        assert_eq!(block[0], Vec4::new(1.0, 2.0, 3.0, 0.5));
    }

    #[test]
    fn write_refuses_unknown_names_and_other_types() {
        let r = reflection_with(&[("coat", Value::Float(0.75))]);
        let mut block = pack(&r).unwrap();
        let err = write(&mut block, &r, "nope", &Value::Float(1.0)).unwrap_err();
        assert_eq!(err.to_string(), "no uniform `nope` in the reflection");
        let err = write(&mut block, &r, "coat", &Value::Integer(1)).unwrap_err();
        assert_eq!(err.to_string(), "uniform `coat` is Float, not Integer");
        assert_eq!(block, pack(&r).unwrap());
    }

    #[test]
    fn oversized_block_is_an_error() {
        let many: Vec<(String, Value)> = (0..65).map(|i| (format!("c{i}"), Value::Color4([1.0; 4]))).collect();
        let refs: Vec<(&str, Value)> = many.iter().map(|(n, v)| (n.as_str(), v.clone())).collect();
        let r = reflection_with(&refs);
        assert!(r.uniform_block_size > UNIFORM_BYTES);
        let err = pack(&r).unwrap_err();
        assert!(matches!(err, ShadewireError::UniformsTooLarge { size: 1040, limit: 1024 }), "{err}");
    }
}
