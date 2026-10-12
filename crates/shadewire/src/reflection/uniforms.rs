use crate::Type;

/// WGSL address-space layout for the uniform block (booleans and integers are 4-byte scalars).
pub fn wgsl_layout(ty: Type) -> (u32, u32) {
    match ty {
        Type::Boolean | Type::Integer | Type::Float => (4, 4),
        Type::Vector2 => (8, 8),
        Type::Vector3 | Type::Color3 => (16, 12),
        Type::Vector4 | Type::Color4 => (16, 16),
        Type::Matrix33 => (16, 48),
        Type::Matrix44 => (16, 64),
    }
}

pub struct UniformLayout {
    pub offsets: Vec<u32>,
    pub block_size: u32,
}

pub fn pack(members: &[(String, Type)]) -> UniformLayout {
    let mut offset = 0u32;
    let mut offsets = Vec::with_capacity(members.len());
    for (_, ty) in members {
        let (align, size) = wgsl_layout(*ty);
        offset = offset.div_ceil(align) * align;
        offsets.push(offset);
        offset += size;
    }
    UniformLayout { offsets, block_size: offset.div_ceil(16) * 16 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Type;

    #[test]
    fn wgsl_uniform_layout() {
        assert_eq!(wgsl_layout(Type::Float), (4, 4));
        assert_eq!(wgsl_layout(Type::Vector2), (8, 8));
        assert_eq!(wgsl_layout(Type::Color3), (16, 12));
        assert_eq!(wgsl_layout(Type::Color4), (16, 16));
        assert_eq!(wgsl_layout(Type::Boolean), (4, 4)); // stored as u32
    }

    #[test]
    fn packs_float_after_vec3() {
        let layout = pack(&[("tint".into(), Type::Color3), ("coat_weight".into(), Type::Float)]);
        assert_eq!(layout.offsets, vec![0, 12]);
        assert_eq!(layout.block_size, 16);
    }
}
