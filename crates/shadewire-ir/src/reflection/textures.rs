use serde::Serialize;

use crate::{AddressMode, Dag, FilterMode};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TextureReflection {
    pub index: usize,
    pub binding: u32,
    pub sampler_binding: u32,
    pub file: String,
    pub colorspace: Option<String>,
    pub address: [AddressMode; 2],
    pub filter: FilterMode,
}

/// Each texture takes two bindings (texture, sampler) starting at `first`.
pub fn textures(dag: &Dag, first: u32) -> Vec<TextureReflection> {
    dag.textures
        .iter()
        .enumerate()
        .map(|(index, t)| TextureReflection {
            index,
            binding: first + 2 * index as u32,
            sampler_binding: first + 2 * index as u32 + 1,
            file: t.file.clone(),
            colorspace: t.colorspace.clone(),
            address: [t.address_u, t.address_v],
            filter: t.filter,
        })
        .collect()
}

/// Approximations the texture table forces on the generated code.
pub fn approximations(dag: &Dag) -> Vec<String> {
    let mut out = Vec::new();
    if dag.textures.iter().any(|t| t.filter == FilterMode::Cubic) {
        out.push("cubic filter as linear".to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TextureDesc;

    fn desc(filter: FilterMode) -> TextureDesc {
        TextureDesc { file: "a.png".into(), colorspace: None, address_u: AddressMode::Periodic, address_v: AddressMode::Clamp, filter }
    }

    #[test]
    fn cubic_filter_is_reported_once() {
        let mut dag = Dag::new("m");
        dag.add_texture(desc(FilterMode::Cubic));
        let mut other = desc(FilterMode::Cubic);
        other.file = "b.png".into();
        dag.add_texture(other);
        assert_eq!(approximations(&dag), vec!["cubic filter as linear".to_string()]);
        let mut linear = Dag::new("m");
        linear.add_texture(desc(FilterMode::Linear));
        assert!(approximations(&linear).is_empty());
    }

    #[test]
    fn bindings_come_in_pairs() {
        let mut dag = Dag::new("m");
        dag.add_texture(desc(FilterMode::Linear));
        let mut other = desc(FilterMode::Linear);
        other.file = "b.png".into();
        dag.add_texture(other);
        let t = textures(&dag, 3);
        assert_eq!((t[0].binding, t[0].sampler_binding), (3, 4));
        assert_eq!((t[1].binding, t[1].sampler_binding), (5, 6));
        assert_eq!(t[0].address, [AddressMode::Periodic, AddressMode::Clamp]);
    }
}
