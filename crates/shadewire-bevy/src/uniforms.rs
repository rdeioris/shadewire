//! Packs a reflection's uniform block into the extension's fixed `[Vec4; 64]` binding.

use bevy::math::Vec4;
use shadewire::{Reflection, Value};

use crate::error::ShadewireError;

/// Bytes in the extension's uniform binding (`[Vec4; 64]`).
pub const UNIFORM_BYTES: u32 = 1024;

/// Every reflection uniform written at its byte offset: f32 as is, i32 and booleans (u32) as bits.
pub fn pack(reflection: &Reflection) -> Result<[Vec4; 64], ShadewireError> {
    if reflection.uniform_block_size > UNIFORM_BYTES {
        return Err(ShadewireError::UniformsTooLarge { size: reflection.uniform_block_size, limit: UNIFORM_BYTES });
    }
    let mut words = [0u32; 256];
    for uniform in &reflection.uniforms {
        let first = (uniform.offset / 4) as usize;
        for (i, bits) in value_words(&uniform.default).into_iter().enumerate() {
            words[first + i] = bits;
        }
    }
    let mut block = [Vec4::ZERO; 64];
    for (i, v) in block.iter_mut().enumerate() {
        let w = &words[i * 4..i * 4 + 4];
        *v = Vec4::new(f32::from_bits(w[0]), f32::from_bits(w[1]), f32::from_bits(w[2]), f32::from_bits(w[3]));
    }
    Ok(block)
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
    fn oversized_block_is_an_error() {
        let many: Vec<(String, Value)> = (0..65).map(|i| (format!("c{i}"), Value::Color4([1.0; 4]))).collect();
        let refs: Vec<(&str, Value)> = many.iter().map(|(n, v)| (n.as_str(), v.clone())).collect();
        let r = reflection_with(&refs);
        assert!(r.uniform_block_size > UNIFORM_BYTES);
        let err = pack(&r).unwrap_err();
        assert!(matches!(err, ShadewireError::UniformsTooLarge { size: 1040, limit: 1024 }), "{err}");
    }
}
