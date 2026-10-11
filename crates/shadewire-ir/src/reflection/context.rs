use serde::Serialize;

use crate::{ContextClass, ContextRead, Dag, NodeKind, Op, TransformKind, Type};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GeomPropReflection {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
    pub function: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ContextReflection {
    pub varyings: Vec<String>,
    pub derived: Vec<String>,
    pub globals: Vec<String>,
    pub geomprops: Vec<GeomPropReflection>,
    pub transforms: Vec<String>,
}

fn kind_name(kind: TransformKind) -> &'static str {
    match kind {
        TransformKind::Point => "point",
        TransformKind::Vector => "vector",
        TransformKind::Normal => "normal",
    }
}

fn sorted_unique(mut names: Vec<String>) -> Vec<String> {
    names.sort();
    names.dedup();
    names
}

pub fn context(dag: &Dag, live: &[bool]) -> ContextReflection {
    let mut varyings = Vec::new();
    let mut derived = Vec::new();
    let mut globals = Vec::new();
    let mut geomprops: Vec<GeomPropReflection> = Vec::new();
    let mut transforms = Vec::new();
    for (i, node) in dag.nodes.iter().enumerate() {
        if !live[i] {
            continue;
        }
        match &node.kind {
            NodeKind::Context(read) => match read.class() {
                ContextClass::Varying => varyings.push(read.reflection_name()),
                ContextClass::Derived => derived.push(read.reflection_name()),
                ContextClass::Global => globals.push(read.reflection_name()),
                ContextClass::GeomProp => {
                    if let ContextRead::GeomProp { name, ty } = read {
                        if !geomprops.iter().any(|g| g.name == *name) {
                            geomprops.push(GeomPropReflection { name: name.clone(), ty: *ty, function: read.function_name() });
                        }
                    }
                }
            },
            NodeKind::Op { op: Op::Transform { kind, from, to }, .. } => {
                transforms.push(format!("transform_{}_{}_to_{}", kind_name(*kind), from.name(), to.name()));
            }
            _ => {}
        }
    }
    geomprops.sort_by(|a, b| a.name.cmp(&b.name));
    ContextReflection {
        varyings: sorted_unique(varyings),
        derived: sorted_unique(derived),
        globals: sorted_unique(globals),
        geomprops,
        transforms: sorted_unique(transforms),
    }
}
