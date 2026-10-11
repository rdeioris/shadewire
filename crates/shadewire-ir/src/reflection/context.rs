use std::collections::BTreeMap;

use serde::Serialize;

use crate::{ContextClass, ContextRead, Dag, NodeKind, Op, Type, Value, transform_name};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GeomPropReflection {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
    pub function: String,
    /// The authored `default` of the `geompropvalue`, when it had one.
    pub default: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ContextReflection {
    pub varyings: Vec<String>,
    pub derived: Vec<String>,
    pub globals: Vec<String>,
    pub geomprops: Vec<GeomPropReflection>,
    pub transforms: Vec<String>,
    /// Type of every varying, derived and global read by reflection name, for hosts that
    /// declare them. Not part of the JSON.
    #[serde(skip)]
    pub types: BTreeMap<String, Type>,
}

fn sorted_unique(mut names: Vec<String>) -> Vec<String> {
    names.sort();
    names.dedup();
    names
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

pub fn context(dag: &Dag, live: &[bool]) -> ContextReflection {
    let mut varyings = Vec::new();
    let mut derived = Vec::new();
    let mut globals = Vec::new();
    let mut geomprops: Vec<GeomPropReflection> = Vec::new();
    let mut transforms = Vec::new();
    let mut types = BTreeMap::new();
    for (i, node) in dag.nodes.iter().enumerate() {
        if !live[i] {
            continue;
        }
        match &node.kind {
            NodeKind::Context(read) => {
                types.insert(read.reflection_name(), read.ty());
                match read.class() {
                    ContextClass::Varying => varyings.push(read.reflection_name()),
                    ContextClass::Derived => derived.push(read.reflection_name()),
                    ContextClass::Global => globals.push(read.reflection_name()),
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
    ContextReflection {
        varyings: sorted_unique(varyings),
        derived: sorted_unique(derived),
        globals: sorted_unique(globals),
        geomprops,
        transforms: sorted_unique(transforms),
        types,
    }
}
