//! The op-to-category catalog: the inverse of `shadewire::op_for_category`.

use shadewire::{Op, Type, input_names};
use shadewire_nodes::Index;
use shadewire_nodes::document::{NodeDef, PortType};

/// The categories Shadewire defines itself; no standard-library nodedef exists for them.
pub const SHADEWIRE_CATEGORIES: [&str; 6] = [
    "shadewire_dpdx",
    "shadewire_dpdy",
    "shadewire_hextile_coord",
    "shadewire_hextile_grad",
    "shadewire_hextile_weights",
    "shadewire_image_grad",
];

/// Whether `name` is one of Shadewire's own categories rather than a standard-library one.
pub fn is_shadewire_category(name: &str) -> bool {
    SHADEWIRE_CATEGORIES.contains(&name)
}

/// The MaterialX category an op is written as, and the string/integer inputs that op parameters become.
#[derive(Clone, Debug, PartialEq)]
pub struct Category {
    /// The node category.
    pub name: String,
    /// `(input name, value text, mtlx type)` for each op parameter.
    pub params: Vec<(&'static str, String, &'static str)>,
}

fn plain(name: &str) -> Category {
    Category { name: name.to_string(), params: Vec::new() }
}

/// The category `op` is written as when it has `arg_count` arguments, with its parameters as typed inputs.
pub fn category_for(op: &Op, arg_count: usize) -> Category {
    match op {
        Op::Add => plain("add"),
        Op::Subtract => plain("subtract"),
        Op::Multiply => plain("multiply"),
        Op::Divide => plain("divide"),
        Op::Modulo => plain("modulo"),
        Op::Invert => plain("invert"),
        Op::Absval => plain("absval"),
        Op::Sign => plain("sign"),
        Op::Floor => plain("floor"),
        Op::Ceil => plain("ceil"),
        Op::Round => plain("round"),
        Op::Fract => plain("fract"),
        Op::Power => plain("power"),
        Op::SafePower => plain("safepower"),
        Op::Sqrt => plain("sqrt"),
        Op::Exp => plain("exp"),
        Op::Ln => plain("ln"),
        Op::Min => plain("min"),
        Op::Max => plain("max"),
        Op::Clamp => plain("clamp"),
        Op::Mix => plain("mix"),
        Op::Smoothstep => plain("smoothstep"),
        Op::Remap => plain("remap"),
        Op::Sin => plain("sin"),
        Op::Cos => plain("cos"),
        Op::Tan => plain("tan"),
        Op::Asin => plain("asin"),
        Op::Acos => plain("acos"),
        Op::Atan2 => plain("atan2"),
        Op::Normalize => plain("normalize"),
        Op::Magnitude => plain("magnitude"),
        Op::Distance => plain("distance"),
        Op::DotProduct => plain("dotproduct"),
        Op::CrossProduct => plain("crossproduct"),
        Op::Rotate2d => plain("rotate2d"),
        Op::Transform { kind, from, to } => Category {
            name: format!("transform{}", kind.name()),
            params: vec![
                ("fromspace", from.name().to_string(), "string"),
                ("tospace", to.name().to_string(), "string"),
            ],
        },
        Op::Greater | Op::IfGreater => plain("ifgreater"),
        Op::GreaterEq | Op::IfGreaterEq => plain("ifgreatereq"),
        Op::Equal | Op::IfEqual => plain("ifequal"),
        Op::Switch => plain("switch"),
        Op::And => plain("and"),
        Op::Or => plain("or"),
        Op::Not => plain("not"),
        Op::Extract(index) => {
            Category { name: "extract".to_string(), params: vec![("index", index.to_string(), "integer")] }
        }
        Op::Combine => Category { name: format!("combine{arg_count}"), params: Vec::new() },
        Op::Convert => plain("convert"),
        Op::Luminance => plain("luminance"),
        Op::RgbToHsv => plain("rgbtohsv"),
        Op::HsvToRgb => plain("hsvtorgb"),
        Op::Noise2d => plain("noise2d"),
        Op::Noise3d => plain("noise3d"),
        Op::Fractal2d => plain("fractal2d"),
        Op::Fractal3d => plain("fractal3d"),
        Op::CellNoise2d => plain("cellnoise2d"),
        Op::CellNoise3d => plain("cellnoise3d"),
        Op::WorleyNoise2d => plain("worleynoise2d"),
        Op::WorleyNoise3d => plain("worleynoise3d"),
        Op::NormalMap => plain("normalmap"),
        Op::HextileCoord(tile) => Category {
            name: "shadewire_hextile_coord".to_string(),
            params: vec![("tile", tile.to_string(), "integer")],
        },
        Op::HextileGrad(tile) => {
            Category { name: "shadewire_hextile_grad".to_string(), params: vec![("tile", tile.to_string(), "integer")] }
        }
        Op::HextileWeights => plain("shadewire_hextile_weights"),
        Op::Dpdx => plain("shadewire_dpdx"),
        Op::Dpdy => plain("shadewire_dpdy"),
    }
}

/// The type of a nodedef's first output, when it is a value.
fn output_type(nodedef: &NodeDef) -> Option<Type> {
    match nodedef.outputs.first()?.port_type {
        PortType::Value(t) => Some(t),
        _ => None,
    }
}

/// The types of a nodedef's value inputs, in order.
fn value_input_types(nodedef: &NodeDef) -> Vec<Type> {
    nodedef
        .inputs
        .iter()
        .filter_map(|p| match p.port_type {
            PortType::Value(t) => Some(t),
            _ => None,
        })
        .collect()
}

/// Whether every argument type is accepted by the matching input type.
fn accepts(inputs: &[Type], arg_types: &[Type], fits: impl Fn(Type, Type) -> bool) -> bool {
    arg_types.len() <= inputs.len() && inputs.iter().zip(arg_types).all(|(input, arg)| fits(*input, *arg))
}

/// The library nodedef for `category` with output `out` whose value inputs, in order, accept `arg_types` (same
/// type or the same width); the first candidate with a matching output when none fits the inputs.
pub fn nodedef_for<'a>(index: &Index<'a>, category: &str, out: Type, arg_types: &[Type]) -> Option<&'a NodeDef> {
    let all = index.nodedefs_for_category(category);
    let mut candidates: Vec<&'a NodeDef> = all.iter().copied().filter(|nd| output_type(nd) == Some(out)).collect();
    if candidates.is_empty() {
        candidates = all
            .iter()
            .copied()
            .filter(|nd| output_type(nd).is_some_and(|t| t.component_count() == out.component_count()))
            .collect();
    }
    let fitting = |fits: fn(Type, Type) -> bool| {
        candidates.iter().copied().find(|nd| accepts(&value_input_types(nd), arg_types, fits))
    };
    fitting(|input, arg| input == arg)
        .or_else(|| fitting(|input, arg| input.component_count() == arg.component_count()))
        .or_else(|| candidates.first().copied())
}

/// The names of `op`'s args as the nodedef spells them: `input_names(op)` filtered to the inputs `nd` has
/// (what `Elaborator::leaf` did when it built the args), zipped with the args.
pub fn arg_names(op: &Op, nd: &NodeDef) -> Vec<&'static str> {
    input_names(op).iter().filter(|n| nd.inputs.iter().any(|p| p.name == **n)).copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shadewire::{Op, Space, TransformKind, Type, op_for_category};
    use shadewire_nodes::Index;
    use shadewire_nodes::document::{PortType, empty_document};
    use shadewire_nodes::library::standard;

    /// Every op, with a representative type and argument count the standard library has a nodedef for.
    const OPS: &[(Op, Type, usize)] = &[
        (Op::Add, Type::Float, 2),
        (Op::Subtract, Type::Color3, 2),
        (Op::Multiply, Type::Vector3, 2),
        (Op::Divide, Type::Float, 2),
        (Op::Modulo, Type::Float, 2),
        (Op::Invert, Type::Color3, 2),
        (Op::Absval, Type::Float, 1),
        (Op::Sign, Type::Float, 1),
        (Op::Floor, Type::Float, 1),
        (Op::Ceil, Type::Float, 1),
        (Op::Round, Type::Float, 1),
        (Op::Fract, Type::Float, 1),
        (Op::Power, Type::Float, 2),
        (Op::SafePower, Type::Float, 2),
        (Op::Sqrt, Type::Float, 1),
        (Op::Exp, Type::Float, 1),
        (Op::Ln, Type::Float, 1),
        (Op::Min, Type::Float, 2),
        (Op::Max, Type::Float, 2),
        (Op::Clamp, Type::Float, 3),
        (Op::Mix, Type::Color3, 3),
        (Op::Smoothstep, Type::Float, 3),
        (Op::Remap, Type::Float, 5),
        (Op::Sin, Type::Float, 1),
        (Op::Cos, Type::Float, 1),
        (Op::Tan, Type::Float, 1),
        (Op::Asin, Type::Float, 1),
        (Op::Acos, Type::Float, 1),
        (Op::Atan2, Type::Float, 2),
        (Op::Normalize, Type::Vector3, 1),
        (Op::Magnitude, Type::Float, 1),
        (Op::Distance, Type::Float, 2),
        (Op::DotProduct, Type::Float, 2),
        (Op::CrossProduct, Type::Vector3, 2),
        (Op::Rotate2d, Type::Vector2, 2),
        (Op::Greater, Type::Boolean, 2),
        (Op::GreaterEq, Type::Boolean, 2),
        (Op::Equal, Type::Boolean, 2),
        (Op::IfGreater, Type::Float, 4),
        (Op::IfGreaterEq, Type::Float, 4),
        (Op::IfEqual, Type::Float, 4),
        (Op::Switch, Type::Float, 11),
        (Op::And, Type::Boolean, 2),
        (Op::Or, Type::Boolean, 2),
        (Op::Not, Type::Boolean, 1),
        (Op::Extract(1), Type::Float, 1),
        (Op::Combine, Type::Vector3, 3),
        (Op::Convert, Type::Color4, 1),
        (Op::Luminance, Type::Color3, 2),
        (Op::RgbToHsv, Type::Color3, 1),
        (Op::HsvToRgb, Type::Color3, 1),
        (Op::Noise2d, Type::Float, 3),
        (Op::Noise3d, Type::Float, 3),
        (Op::Fractal2d, Type::Float, 5),
        (Op::Fractal3d, Type::Float, 5),
        (Op::CellNoise2d, Type::Float, 1),
        (Op::CellNoise3d, Type::Float, 1),
        (Op::WorleyNoise2d, Type::Float, 3),
        (Op::WorleyNoise3d, Type::Float, 3),
        (Op::NormalMap, Type::Vector3, 5),
        (Op::Transform { kind: TransformKind::Point, from: Space::Object, to: Space::World }, Type::Vector3, 1),
    ];

    #[test]
    fn every_library_op_round_trips_through_its_category() {
        let doc = empty_document();
        let index = Index::new(doc, standard());
        for (op, ty, args) in OPS {
            let category = category_for(op, *args);
            assert!(!is_shadewire_category(&category.name), "{op:?}");
            let arg_types = vec![*ty; *args];
            let nd = nodedef_for(&index, &category.name, *ty, &arg_types)
                .unwrap_or_else(|| panic!("{op:?}: no nodedef for {} {}", category.name, ty.mtlx_name()));
            let value_inputs = nd.inputs.iter().filter(|p| matches!(p.port_type, PortType::Value(_))).count();
            let back = op_for_category(&category.name, *ty, value_inputs).unwrap_or_else(|| panic!("{op:?}"));
            // Extract and Transform carry parameters the category alone does not; compare the rest exactly.
            match op {
                Op::Extract(_) => assert!(matches!(back, Op::Extract(_))),
                Op::Transform { kind, .. } => assert!(matches!(back, Op::Transform { kind: k, .. } if k == *kind)),
                other => assert_eq!(&back, other, "{}", category.name),
            }
            assert_eq!(arg_names(op, nd).len(), *args, "{op:?} on {}", nd.name);
        }
    }

    #[test]
    fn parameters_become_typed_inputs() {
        let c = category_for(&Op::Extract(2), 1);
        assert_eq!((c.name.as_str(), &c.params[..]), ("extract", &[("index", "2".to_string(), "integer")][..]));
        let t = category_for(&Op::Transform { kind: TransformKind::Normal, from: Space::Object, to: Space::World }, 1);
        assert_eq!(t.name, "transformnormal");
        assert_eq!(
            t.params,
            vec![("fromspace", "object".to_string(), "string"), ("tospace", "world".to_string(), "string")]
        );
        assert_eq!(category_for(&Op::Combine, 4).name, "combine4");
        let h = category_for(&Op::HextileCoord(2), 7);
        assert!(is_shadewire_category(&h.name));
        assert_eq!(h.params, vec![("tile", "2".to_string(), "integer")]);
    }

    #[test]
    fn switch_picks_the_nodedef_by_its_which_type() {
        let doc = empty_document();
        let index = Index::new(doc, standard());
        let float_which = nodedef_for(&index, "switch", Type::Float, &[Type::Float; 11]).unwrap();
        let mut int_args = [Type::Float; 11];
        int_args[10] = Type::Integer;
        let int_which = nodedef_for(&index, "switch", Type::Float, &int_args).unwrap();
        assert_ne!(float_which.name, int_which.name);
        assert!(int_which.name.ends_with('I'), "{}", int_which.name);
    }
}
