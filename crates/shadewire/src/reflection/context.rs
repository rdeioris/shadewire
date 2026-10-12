use std::collections::BTreeMap;

use serde::Serialize;

use crate::{ContextClass, ContextRead, Dag, Error, Location, NodeKind, Op, Type, Value, transform_name};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GeomPropReflection {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
    pub function: String,
    /// The authored `default` of the `geompropvalue`, when it had one.
    pub default: Option<Value>,
}

/// One varying, derived or global read: its reflection name (the host function
/// is `mx_` + name) and the type the function returns.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ContextEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ContextReflection {
    pub varyings: Vec<ContextEntry>,
    pub derived: Vec<ContextEntry>,
    pub globals: Vec<ContextEntry>,
    pub geomprops: Vec<GeomPropReflection>,
    pub transforms: Vec<String>,
}

fn sorted_unique(mut names: Vec<String>) -> Vec<String> {
    names.sort();
    names.dedup();
    names
}

fn sorted_entries(mut entries: Vec<ContextEntry>) -> Vec<ContextEntry> {
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries.dedup_by(|a, b| a.name == b.name);
    entries
}

/// Authored names may hold characters a function name cannot, and two names can sanitise
/// alike: replace those characters with `_` and suffix `_2`, `_3`... on a clash.
fn unique_functions(geomprops: &mut [GeomPropReflection]) {
    let mut taken: Vec<String> = Vec::new();
    for g in geomprops.iter_mut() {
        let clean: String =
            g.name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
        let base = format!("mx_geomprop_{clean}");
        let mut candidate = base.clone();
        let mut n = 2;
        while taken.contains(&candidate) {
            candidate = format!("{base}_{n}");
            n += 1;
        }
        taken.push(candidate.clone());
        g.function = candidate;
    }
}

/// The context reads of the live nodes. One host function returns one type, so
/// two reads with one reflection name and different types are a type mismatch.
pub fn context(dag: &Dag, live: &[bool]) -> Result<ContextReflection, Error> {
    let mut varyings = Vec::new();
    let mut derived = Vec::new();
    let mut globals = Vec::new();
    let mut geomprops: Vec<GeomPropReflection> = Vec::new();
    let mut transforms = Vec::new();
    let mut types: BTreeMap<String, Type> = BTreeMap::new();
    for (i, node) in dag.nodes.iter().enumerate() {
        if !live[i] {
            continue;
        }
        match &node.kind {
            NodeKind::Context(read) => {
                let name = read.reflection_name();
                let ty = read.ty();
                match types.get(&name) {
                    Some(first) if *first != ty => {
                        return Err(Error::TypeMismatch {
                            at: Location { file: None, line: 0, path: node.origin.clone() },
                            expected: first.mtlx_name().to_string(),
                            found: ty.mtlx_name().to_string(),
                        });
                    }
                    _ => {
                        types.insert(name.clone(), ty);
                    }
                }
                let entry = ContextEntry { name, ty };
                match read.class() {
                    ContextClass::Varying => varyings.push(entry),
                    ContextClass::Derived => derived.push(entry),
                    ContextClass::Global => globals.push(entry),
                    ContextClass::GeomProp => {
                        if let ContextRead::GeomProp { name, ty } = read {
                            if !geomprops.iter().any(|g| g.name == *name) {
                                geomprops.push(GeomPropReflection {
                                    name: name.clone(),
                                    ty: *ty,
                                    function: read.function_name(),
                                    default: dag.geomprop_defaults.get(name).cloned(),
                                });
                            }
                        }
                    }
                }
            }
            NodeKind::Op { op: Op::Transform { kind, from, to }, .. } => {
                transforms.push(transform_name(*kind, *from, *to));
            }
            _ => {}
        }
    }
    geomprops.sort_by(|a, b| a.name.cmp(&b.name));
    unique_functions(&mut geomprops);
    Ok(ContextReflection {
        varyings: sorted_entries(varyings),
        derived: sorted_entries(derived),
        globals: sorted_entries(globals),
        geomprops,
        transforms: sorted_unique(transforms),
    })
}

#[cfg(test)]
mod tests {
    use super::{ContextEntry, context};
    use crate::*;

    fn read(dag: &mut Dag, read: ContextRead, origin: &str) {
        let ty = read.ty();
        let id = dag.add(Node { kind: NodeKind::Context(read), ty, origin: origin.into() });
        dag.surface.fields.push(SurfaceField { name: origin.into(), ty, node: id });
    }

    #[test]
    fn entries_carry_their_type() {
        let mut dag = Dag::new("m");
        read(&mut dag, ContextRead::TexCoord { index: 1, ty: Type::Vector3 }, "a");
        read(&mut dag, ContextRead::TexCoord { index: 1, ty: Type::Vector3 }, "b");
        let c = context(&dag, &[true, true]).unwrap();
        assert_eq!(c.varyings, vec![ContextEntry { name: "texcoord_1".into(), ty: Type::Vector3 }]);
        let json = serde_json::to_string(&c.varyings).unwrap();
        assert_eq!(json, r#"[{"name":"texcoord_1","type":"vector3"}]"#);
    }

    #[test]
    fn one_name_at_two_types_is_a_mismatch() {
        let mut dag = Dag::new("m");
        read(&mut dag, ContextRead::GeomProp { name: "tint".into(), ty: Type::Color3 }, "a");
        read(&mut dag, ContextRead::GeomProp { name: "tint".into(), ty: Type::Float }, "b");
        match context(&dag, &[true, true]) {
            Err(Error::TypeMismatch { at, expected, found }) => {
                assert_eq!((at.path.as_str(), expected.as_str(), found.as_str()), ("b", "color3", "float"));
            }
            other => panic!("expected a type mismatch, got {other:?}"),
        }
    }
}
