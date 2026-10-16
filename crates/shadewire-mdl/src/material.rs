//! A material: its parameters as uniforms, evaluated, and its distribution functions mapped onto pbrlib lobes
//! finished by `shadewire-nodes` as a closure tree.

use std::rc::Rc;

use shadewire::{ContextRead, Error, NodeKind, Op, SourceInfo, Space, Type, UiHints, Value};
use shadewire_nodes::document::PortType;
use shadewire_nodes::elaborate::STACK_BYTES;
use shadewire_nodes::{Arg, Elaborator, IrOptions, MaterialIr, ShaderNode, Synthetic};

use crate::ast::{Annotation, DeclKind, Param};
use crate::eval::{Eval, Flow, Frame};
use crate::modules::{ArgValue, Argument, Modules};
use crate::program::{Program, Resolver};
use crate::types::{TextureShape, Ty};
use crate::value::{Df, Gamma, Num, Texture, V, Val};

/// The exported materials of the module in `text`.
pub fn material_names(text: &str, location: Option<&str>, resolver: &Resolver) -> Result<Vec<String>, Error> {
    on_reader_stack(|| {
        let modules = Modules::new(resolver.clone())?;
        let main = modules.add_main(text, location)?;
        Ok(modules.materials(main))
    })
}

/// Material `material` of the module in `text` (its only one when `None`) as IR for `options`.
pub fn to_ir(
    text: &str,
    location: Option<&str>,
    material: Option<&str>,
    options: &IrOptions,
    resolver: &Resolver,
) -> Result<MaterialIr, Error> {
    on_reader_stack(|| {
        let modules = Modules::new(resolver.clone())?;
        let main = modules.add_main(text, location)?;
        modules.lower(main, material, &[], options)
    })
}

/// The name of the threads `on_reader_stack` starts.
const READER: &str = "shadewire-mdl";

/// Runs `run` on a thread with the elaborator's stack (`STACK_BYTES`), which parsing, evaluating and dropping a module
/// nested up to `MAX_NESTING` fit in; on this thread when it is one already, or when no thread can be started.
pub(crate) fn on_reader_stack<T: Send>(run: impl Fn() -> T + Send + Sync) -> T {
    if std::thread::current().name() == Some(READER) {
        return run();
    }
    std::thread::scope(|threads| {
        let builder = std::thread::Builder::new().name(READER.to_string()).stack_size(STACK_BYTES);
        match builder.spawn_scoped(threads, &run) {
            Ok(handle) => match handle.join() {
                Ok(result) => result,
                Err(panic) => std::panic::resume_unwind(panic),
            },
            Err(_) => run(),
        }
    })
}

/// Material `material` of module `main` (its only one when `None`) as IR for `options`, `arguments` in place of
/// parameter defaults.
pub(crate) fn to_ir_here(
    program: &Program,
    main: usize,
    material: Option<&str>,
    arguments: &[Argument],
    options: &IrOptions,
) -> Result<MaterialIr, Error> {
    let names = program.materials(main);
    let name = match material {
        Some(m) => {
            names.iter().find(|n| *n == m).cloned().ok_or_else(|| Error::UnknownMaterial { name: m.to_string() })?
        }
        None => match names.as_slice() {
            [] => return Err(Error::NoMaterial),
            [one] => one.clone(),
            _ => return Err(Error::AmbiguousMaterial { names }),
        },
    };
    let decl = program.modules[main]
        .ast
        .decls
        .iter()
        .position(|d| d.export && matches!(&d.kind, DeclKind::Function(f) if f.name == name))
        .expect("a listed material is declared");
    let synthetic = Synthetic::new();
    let e = Elaborator::standalone(&synthetic, options);
    let label = format!("{}::{name}", program.modules[main].name.trim_start_matches("::"));
    let mut ev = Eval::new(program, e, main, &label);
    let args = parameters(&mut ev, main, decl, arguments)?;
    let value = ev.invoke(main, decl, args.clone(), Default::default(), 0)?;
    // The geometry's normal is the shading normal the surface sees: evaluate again with it in place.
    let normal = ev.field(&value, "geometry").and_then(|g| ev.field(g, "normal")).cloned();
    let value = match normal.as_ref().and_then(|n| n.num().cloned()) {
        Some(n) if Some(&n) != ev.default_normal().as_ref() => {
            let id = ev.node_of(&n, "geometry.normal");
            ev.normal = Some(id);
            ev.invoke(main, decl, args, Default::default(), 0)?
        }
        _ => value,
    };
    let finished = finish(&mut ev, &value)?;
    let Eval { e, warnings, .. } = ev;
    let (nodes, surface) = finished;
    let dag = e.finish_closure(nodes, &surface)?;
    let version = format!("MDL {}.{}", program.modules[main].version().0, program.modules[main].version().1);
    let source = SourceInfo { source: "mdl".to_string(), mtlx_version: version };
    Ok(MaterialIr { dag, source, warnings })
}

/// `anno::hard_range` and `anno::soft_range` as limits, `anno::in_group` as a folder.
fn ui_hints(ev: &mut Eval, module: usize, annotations: &[Annotation]) -> UiHints {
    let mut hints = UiHints::default();
    for a in annotations {
        let mut values = Vec::new();
        let mut frame = Frame::new(module, "annotation");
        let mut flow = Flow::new();
        for arg in &a.args {
            match ev.expr(&mut frame, &mut flow, &arg.value) {
                Ok(v) => values.push(v),
                Err(_) => break,
            }
        }
        let known = |v: Option<&Val>| v.and_then(|v| v.known_value().cloned());
        match a.name.last() {
            "hard_range" => {
                hints.min = known(values.first());
                hints.max = known(values.get(1));
            }
            "soft_range" => {
                hints.soft_min = known(values.first());
                hints.soft_max = known(values.get(1));
            }
            "in_group" => {
                let parts: Vec<&str> = values.iter().filter_map(Val::str).filter(|s| !s.is_empty()).collect();
                if !parts.is_empty() {
                    hints.folder = Some(parts.join("/"));
                }
            }
            _ => {}
        }
    }
    hints
}

/// The material's parameters: each argument, else its default evaluated, and a value of an IR type known now made a
/// uniform (or kept a constant, per the options) named after its parameter. Integers stay constants: MDL uses them
/// for counts and modes, which loops and switches need known. An argument naming no parameter is a warning.
fn parameters(ev: &mut Eval, module: usize, decl: usize, arguments: &[Argument]) -> Result<Vec<Option<Val>>, Error> {
    let func = ev.program.function(module, decl).clone();
    let params = func.params.as_deref().unwrap_or_default();
    for a in arguments {
        if !params.iter().any(|p| p.name == a.name) {
            ev.warn(format!("no parameter {}", a.name));
        }
    }
    let mut frame = Frame::new(module, &func.name);
    let mut flow = Flow::new();
    let mut out = Vec::new();
    for p in params {
        let ty = ev.resolve_type(&mut frame, &p.ty, p.line)?;
        let v = match arguments.iter().find(|a| a.name == p.name) {
            Some(a) => argument_val(ev, module, &func.name, p, ty.as_ref(), &a.value)?,
            None => {
                let Some(default) = &p.default else {
                    return Err(Error::BadValue {
                        at: ev.at(module, p.line, &func.name),
                        message: format!("parameter {} has no default", p.name),
                    });
                };
                let v = ev.expr(&mut frame, &mut flow, default)?;
                match ty {
                    Some(t) => {
                        let t = ev.complete_array(&t, &v);
                        ev.convert(v, &t, false, p.line, module)?
                    }
                    None => v,
                }
            }
        };
        let v = match (v.known_value().cloned(), v.ty.ir()) {
            (Some(value), Some(_)) if v.ty != Ty::INT => {
                let id = ev.e.authored(&p.name, value, &p.name);
                if let NodeKind::Uniform { name, .. } = ev.e.dag.node(id).kind.clone() {
                    let hints = ui_hints(ev, module, &p.annotations);
                    if !hints.is_empty() {
                        ev.e.dag.uniform_ui.insert(name, hints);
                    }
                }
                Val::node(v.ty.clone(), id)
            }
            _ => v,
        };
        frame.declare(&p.name, v.clone());
        out.push(Some(v));
    }
    Ok(out)
}

/// Parameter `p` (of type `ty`) of `function` given `value`: the value its default would have had.
fn argument_val(
    ev: &mut Eval,
    module: usize,
    function: &str,
    p: &Param,
    ty: Option<&Ty>,
    value: &ArgValue,
) -> Result<Val, Error> {
    let bad = |ev: &Eval, message: String| ev.bad(ev.at(module, p.line, function), format!("{}: {message}", p.name));
    let v = match value {
        ArgValue::Value(v) => known_val(v).ok_or_else(|| bad(ev, "cannot take a matrix".to_string()))?,
        ArgValue::Int(i) => {
            if let Some(Ty::Enum(id)) = ty {
                let def = &ev.program.enums[*id];
                return match def.values.iter().find(|(_, v)| i64::from(*v) == *i) {
                    Some((_, v)) => Ok(Val::new(Ty::Enum(*id), V::Enum(*v))),
                    None => Err(bad(ev, format!("{} has no value {i}", def.name))),
                };
            }
            Val::int(i32::try_from(*i).map_err(|_| bad(ev, format!("{i} is out of the int range")))?)
        }
        // What `texture_2d(file, gamma)` evaluates to.
        ArgValue::Texture { file, colorspace } => {
            let shape = match ty {
                Some(Ty::Texture(shape)) => *shape,
                None => TextureShape::Tex2d,
                Some(other) => return Err(bad(ev, format!("a texture for a {}", ev.type_name(other)))),
            };
            let gamma = match colorspace.as_deref() {
                Some(c) if c.eq_ignore_ascii_case("srgb") => Gamma::Srgb,
                Some(c) if c.eq_ignore_ascii_case("raw") => Gamma::Linear,
                _ => Gamma::Default,
            };
            let texture = Texture { shape, file: ev.resolve_resource(file), gamma };
            return Ok(Val::new(Ty::Texture(shape), V::Texture(texture)));
        }
    };
    let Some(ty) = ty else { return Ok(v) };
    let origin = format!("{}:{}", ev.program.modules[module].name, p.line);
    ev.convert_val(v, ty, false, &origin).map_err(|m| bad(ev, m))
}

/// A value as the evaluator holds it; `None` for a matrix, which it holds by column.
fn known_val(v: &Value) -> Option<Val> {
    Some(match v {
        Value::Boolean(b) => Val::bool(*b),
        Value::Integer(i) => Val::int(*i),
        Value::Color4(c) => Val::known(Ty::float_vector(4), Value::Vector4(*c)),
        Value::Matrix33(_) | Value::Matrix44(_) => return None,
        other => Val::known(Ty::of_ir(other.ty()), other.clone()),
    })
}

/// A pbrlib closure tree on its way to `ShaderNode`s.
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
enum Tree {
    Lobe {
        kind: &'static str,
        inputs: Vec<(String, Arg)>,
    },
    /// A microfacet lobe without Fresnel (MDL's elemental glossy BSDFs): a Schlick lobe of one tint unless a
    /// layer above gives it a Fresnel curve.
    Specular {
        tint: Num,
        roughness: [Num; 2],
        tangent: Option<Num>,
        mode: i32,
        normal: Option<Num>,
    },
    Layer(Box<Tree>, Box<Tree>),
    Mix {
        fg: Box<Tree>,
        bg: Box<Tree>,
        weight: Num,
    },
    Add(Box<Tree>, Box<Tree>),
    Multiply(Box<Tree>, Num),
}

/// What lowering a distribution function needs around it: the normal its layers set, the material's ior.
#[derive(Clone)]
struct Around {
    normal: Option<Num>,
    ior: Num,
}

impl<'p, 'a> Eval<'p, 'a> {
    /// The value `state::normal()` has without a geometry normal.
    pub fn default_normal(&mut self) -> Option<Num> {
        Some(self.context(ContextRead::Normal(Space::World), "state::normal"))
    }

    fn arg<'v>(args: &'v [(String, Val)], name: &str) -> Option<&'v Val> {
        args.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    fn num_arg(args: &[(String, Val)], name: &str) -> Option<Num> {
        Self::arg(args, name).and_then(|v| v.num().cloned())
    }

    /// A normal argument: `None` when it is the shading normal anyway.
    fn layer_normal(&mut self, args: &[(String, Val)]) -> Option<Num> {
        let n = Self::num_arg(args, "normal")?;
        let shading = self.shading_normal("state::normal");
        (n != shading).then_some(n)
    }

    fn df_arg(args: &[(String, Val)], name: &str) -> Rc<Df> {
        Self::arg(args, name).and_then(|v| v.df_value().cloned()).unwrap_or_else(|| Rc::new(Df::Default))
    }

    fn mul(&mut self, a: &Num, b: &Num, ty: Type, origin: &str) -> Result<Num, Error> {
        if a.known() == Some(&Value::splat(self.num_type(a), 1.0)) && self.num_type(b) == ty {
            return Ok(b.clone());
        }
        self.emit(Op::Multiply, &[a.clone(), b.clone()], ty, origin)
    }

    /// The scattering tree of a `bsdf` value; `None` scatters nothing.
    fn bsdf_tree(&mut self, df: &Df, around: &Around) -> Result<Option<Tree>, Error> {
        let o = "df";
        let (name, args, line) = match df {
            Df::Default => return Ok(None),
            Df::Select { cond, then, otherwise } => {
                let a = self.bsdf_tree(then, around)?;
                let b = self.bsdf_tree(otherwise, around)?;
                let w = self.emit(Op::Convert, &[Num::Node(*cond)], Type::Float, o)?;
                return mix_trees(a, b, w, self);
            }
            Df::Call { name, args, line } => (name.as_str(), args.as_slice(), *line),
        };
        let at = |_: &Self| format!("df::{name} (line {line})");
        let tint = |a: &[(String, Val)]| Self::num_arg(a, "tint").unwrap_or(Num::Known(Value::Color3([1.0; 3])));
        let normal_input = |inputs: &mut Vec<(String, Arg)>, around: &Around| {
            if let Some(n) = &around.normal {
                inputs.push(("normal".into(), num_arg(n)));
            }
        };
        Ok(match name {
            "diffuse_reflection_bsdf" | "dusty_diffuse_reflection_bsdf" => {
                if name.starts_with("dusty") {
                    self.warn(format!("{}: Oren-Nayar diffuse", at(self)));
                }
                let mut inputs = vec![("color".into(), num_arg(&tint(args)))];
                if let Some(r) = Self::num_arg(args, "roughness") {
                    inputs.push(("roughness".into(), num_arg(&r)));
                }
                // MDL 1.10's multiscatter tint compensates the energy Oren-Nayar loses.
                if let Some(t) = Self::num_arg(args, "multiscatter_tint")
                    && t.known().is_none_or(|v| v.floats().iter().any(|c| *c != 0.0))
                {
                    if t.known() != Some(&Value::Color3([1.0; 3])) {
                        self.warn(format!("{}: a multiscatter tint other than white, as white", at(self)));
                    }
                    inputs.push(("energy_compensation".into(), Arg::Value(Value::Boolean(true))));
                }
                normal_input(&mut inputs, around);
                Some(Tree::Lobe { kind: "oren_nayar_diffuse_bsdf", inputs })
            }
            "diffuse_transmission_bsdf" => {
                let mut inputs = vec![("color".into(), num_arg(&tint(args)))];
                normal_input(&mut inputs, around);
                Some(Tree::Lobe { kind: "translucent_bsdf", inputs })
            }
            "specular_bsdf" => Some(Tree::Specular {
                tint: tint(args),
                roughness: [Num::Known(Value::Float(0.0)), Num::Known(Value::Float(0.0))],
                tangent: None,
                mode: Self::arg(args, "mode").and_then(Val::known_int).unwrap_or(0),
                normal: around.normal.clone(),
            }),
            "simple_glossy_bsdf"
            | "backscattering_glossy_reflection_bsdf"
            | "microfacet_beckmann_smith_bsdf"
            | "microfacet_ggx_smith_bsdf"
            | "microfacet_beckmann_vcavities_bsdf"
            | "microfacet_ggx_vcavities_bsdf"
            | "ward_geisler_moroder_bsdf" => {
                if !name.contains("ggx") {
                    self.warn(format!("{}: a GGX lobe", at(self)));
                }
                let ru = Self::num_arg(args, "roughness_u").unwrap_or(Num::Known(Value::Float(0.0)));
                let rv = Self::num_arg(args, "roughness_v").unwrap_or_else(|| ru.clone());
                let tangent = Self::num_arg(args, "tangent_u");
                let default_tangent = self.context(ContextRead::Tangent(Space::World), "state::texture_tangent_u");
                Some(Tree::Specular {
                    tint: tint(args),
                    roughness: [ru, rv],
                    tangent: tangent.filter(|t| *t != default_tangent),
                    mode: Self::arg(args, "mode").and_then(Val::known_int).unwrap_or(0),
                    normal: around.normal.clone(),
                })
            }
            "sheen_bsdf" | "microflake_sheen_bsdf" => {
                if name.starts_with("microflake") {
                    self.warn(format!("{}: a sheen lobe", at(self)));
                }
                let mut inputs = vec![("color".into(), num_arg(&tint(args)))];
                if let Some(r) = Self::num_arg(args, "roughness") {
                    inputs.push(("roughness".into(), num_arg(&r)));
                }
                normal_input(&mut inputs, around);
                let sheen = Tree::Lobe { kind: "sheen_bsdf", inputs };
                // MDL's sheen scatters what it does not reflect with its multiscatter BSDF.
                match self.bsdf_tree(&Self::df_arg(args, "multiscatter"), around)? {
                    Some(base) => Some(Tree::Layer(Box::new(sheen), Box::new(base))),
                    None => Some(sheen),
                }
            }
            "tint" => {
                let base = self.bsdf_tree(&Self::df_arg(args, "base"), around)?;
                let tint = match Self::num_arg(args, "tint") {
                    Some(t) => t,
                    None => {
                        self.warn(format!("{}: the reflection tint for both", at(self)));
                        Self::num_arg(args, "reflection_tint").unwrap_or(Num::Known(Value::Color3([1.0; 3])))
                    }
                };
                base.map(|b| Tree::Multiply(Box::new(b), tint))
            }
            "thin_film" => {
                let base = self.bsdf_tree(&Self::df_arg(args, "base"), around)?;
                let thickness = Self::num_arg(args, "thickness").unwrap_or(Num::Known(Value::Float(0.0)));
                let ior = Self::num_arg(args, "ior").unwrap_or(Num::Known(Value::Color3([1.5; 3])));
                let ior = self.gray(&ior, &at(self), "thin film ior")?;
                base.map(|b| with_thin_film(b, &thickness, &ior))
            }
            "weighted_layer" | "color_weighted_layer" => {
                let w = Self::num_arg(args, "weight").unwrap_or(Num::Known(Value::Float(1.0)));
                let inner =
                    Around { normal: self.layer_normal(args).or(around.normal.clone()), ior: around.ior.clone() };
                let top = self.bsdf_tree(&Self::df_arg(args, "layer"), &inner)?;
                let base = self.bsdf_tree(&Self::df_arg(args, "base"), around)?;
                let top = top.map(|t| weighted(t, w));
                layered(top, base)
            }
            "fresnel_layer" | "color_fresnel_layer" => {
                let w = Self::num_arg(args, "weight").unwrap_or(Num::Known(Value::Float(1.0)));
                let ior = Self::num_arg(args, "ior").unwrap_or(Num::Known(Value::Float(1.5)));
                let ior = self.gray(&ior, &at(self), "ior")?;
                let inner =
                    Around { normal: self.layer_normal(args).or(around.normal.clone()), ior: around.ior.clone() };
                let top = self.bsdf_tree(&Self::df_arg(args, "layer"), &inner)?;
                let base = self.bsdf_tree(&Self::df_arg(args, "base"), around)?;
                let top = match top {
                    Some(Tree::Specular { tint, roughness, tangent, normal, .. }) => {
                        let (weight, tint) = self.split_weight(&w, &tint)?;
                        let mut inputs = vec![
                            ("weight".into(), num_arg(&weight)),
                            ("tint".into(), num_arg(&tint)),
                            ("ior".into(), num_arg(&ior)),
                            ("roughness".into(), self.roughness(&roughness)?),
                            ("scatter_mode".into(), Arg::Text("R".into())),
                        ];
                        lobe_geometry(&mut inputs, tangent, normal);
                        Some(Tree::Lobe { kind: "dielectric_bsdf", inputs })
                    }
                    Some(other) => {
                        self.warn(format!(
                            "{}: a layer that is not a glossy lobe, weighted without its Fresnel curve",
                            at(self)
                        ));
                        Some(weighted(other, w))
                    }
                    None => None,
                };
                layered(top, base)
            }
            "custom_curve_layer"
            | "color_custom_curve_layer"
            | "measured_curve_layer"
            | "color_measured_curve_layer" => {
                let w = Self::num_arg(args, "weight").unwrap_or(Num::Known(Value::Float(1.0)));
                let (r0, r90, f82, exponent) = if name.contains("measured") {
                    self.warn(format!(
                        "{}: a Schlick curve through the measured curve's first and last values",
                        at(self)
                    ));
                    let curve = Self::arg(args, "curve_values").and_then(Val::elements).unwrap_or(&[]).to_vec();
                    let first =
                        curve.first().and_then(|v| v.num().cloned()).unwrap_or(Num::Known(Value::Color3([1.0; 3])));
                    let last =
                        curve.last().and_then(|v| v.num().cloned()).unwrap_or(Num::Known(Value::Color3([1.0; 3])));
                    (first, last, None, Num::Known(Value::Float(5.0)))
                } else {
                    let r0 = Self::num_arg(args, "normal_reflectivity").unwrap_or(Num::Known(Value::Float(0.0)));
                    let r90 = Self::num_arg(args, "grazing_reflectivity").unwrap_or(Num::Known(Value::Float(1.0)));
                    let f82 = Self::num_arg(args, "f82_factor");
                    let e = Self::num_arg(args, "exponent").unwrap_or(Num::Known(Value::Float(5.0)));
                    (r0, r90, f82, e)
                };
                let inner =
                    Around { normal: self.layer_normal(args).or(around.normal.clone()), ior: around.ior.clone() };
                let top = self.bsdf_tree(&Self::df_arg(args, "layer"), &inner)?;
                let base = self.bsdf_tree(&Self::df_arg(args, "base"), around)?;
                let top = match top {
                    Some(Tree::Specular { tint, roughness, tangent, normal, mode }) => {
                        let (weight, tint) = self.split_weight(&w, &tint)?;
                        let (r0, r90) = (self.as_color(&r0)?, self.as_color(&r90)?);
                        let color0 = self.mul(&r0, &tint, Type::Color3, "df")?;
                        let color90 = self.mul(&r90, &tint, Type::Color3, "df")?;
                        let mut inputs = vec![
                            ("weight".into(), num_arg(&weight)),
                            ("color0".into(), num_arg(&color0)),
                            ("color90".into(), num_arg(&color90)),
                            ("exponent".into(), num_arg(&exponent)),
                            ("roughness".into(), self.roughness(&roughness)?),
                            ("scatter_mode".into(), Arg::Text(scatter_mode(mode).into())),
                        ];
                        if let Some(f82) = f82 {
                            inputs.push(("color82".into(), num_arg(&self.as_color(&f82)?)));
                        }
                        lobe_geometry(&mut inputs, tangent, normal);
                        Some(Tree::Lobe { kind: "generalized_schlick_bsdf", inputs })
                    }
                    Some(other) => {
                        self.warn(format!(
                            "{}: a layer that is not a glossy lobe, weighted by its normal reflectivity",
                            at(self)
                        ));
                        let r0 = self.as_color(&r0)?;
                        let w = self.as_color(&w)?;
                        let k = self.mul(&r0, &w, Type::Color3, "df")?;
                        Some(Tree::Multiply(Box::new(other), k))
                    }
                    None => None,
                };
                layered(top, base)
            }
            "fresnel_factor" | "directional_factor" => {
                let base = self.bsdf_tree(&Self::df_arg(args, "base"), around)?;
                match base {
                    Some(Tree::Specular { tint, roughness, tangent, normal, .. }) if name == "fresnel_factor" => {
                        let ior = Self::num_arg(args, "ior").unwrap_or(Num::Known(Value::Color3([1.5; 3])));
                        let k = Self::num_arg(args, "extinction_coefficient")
                            .unwrap_or(Num::Known(Value::Color3([0.0; 3])));
                        let mut inputs = vec![
                            ("ior".into(), num_arg(&self.as_color(&ior)?)),
                            ("extinction".into(), num_arg(&self.as_color(&k)?)),
                            ("roughness".into(), self.roughness(&roughness)?),
                        ];
                        lobe_geometry(&mut inputs, tangent, normal);
                        let conductor = Tree::Lobe { kind: "conductor_bsdf", inputs };
                        Some(if tint.known() == Some(&Value::Color3([1.0; 3])) {
                            conductor
                        } else {
                            Tree::Multiply(Box::new(conductor), tint)
                        })
                    }
                    Some(Tree::Specular { tint, roughness, tangent, normal, mode }) => {
                        let nt = Self::num_arg(args, "normal_tint").unwrap_or(Num::Known(Value::Color3([1.0; 3])));
                        let gt = Self::num_arg(args, "grazing_tint").unwrap_or(Num::Known(Value::Color3([1.0; 3])));
                        let e = Self::num_arg(args, "exponent").unwrap_or(Num::Known(Value::Float(5.0)));
                        let color0 = self.mul(&nt, &tint, Type::Color3, "df")?;
                        let color90 = self.mul(&gt, &tint, Type::Color3, "df")?;
                        let mut inputs = vec![
                            ("color0".into(), num_arg(&color0)),
                            ("color90".into(), num_arg(&color90)),
                            ("exponent".into(), num_arg(&e)),
                            ("roughness".into(), self.roughness(&roughness)?),
                            ("scatter_mode".into(), Arg::Text(scatter_mode(mode).into())),
                        ];
                        if let Some(f82) = Self::num_arg(args, "f82_factor") {
                            inputs.push(("color82".into(), num_arg(&f82)));
                        }
                        lobe_geometry(&mut inputs, tangent, normal);
                        Some(Tree::Lobe { kind: "generalized_schlick_bsdf", inputs })
                    }
                    Some(other) => {
                        self.warn(format!("{}: over a BSDF that is not a glossy lobe, dropped", at(self)));
                        Some(other)
                    }
                    None => None,
                }
            }
            "normalized_mix"
            | "clamped_mix"
            | "unbounded_mix"
            | "color_normalized_mix"
            | "color_clamped_mix"
            | "color_unbounded_mix" => {
                let components = Self::arg(args, "components").and_then(Val::elements).unwrap_or(&[]).to_vec();
                let mut parts = Vec::new();
                for c in &components {
                    let w =
                        self.field(c, "weight").and_then(|v| v.num().cloned()).unwrap_or(Num::Known(Value::Float(0.0)));
                    let d = self
                        .field(c, "component")
                        .and_then(|v| v.df_value().cloned())
                        .unwrap_or_else(|| Rc::new(Df::Default));
                    parts.push((w, d));
                }
                let weights = self.mix_weights(name, parts.iter().map(|(w, _)| w.clone()).collect())?;
                let mut sum: Option<Tree> = None;
                for ((_, d), w) in parts.iter().zip(weights) {
                    let Some(t) = self.bsdf_tree(d, around)? else { continue };
                    let t = weighted(t, w);
                    sum = Some(match sum {
                        Some(s) => Tree::Add(Box::new(s), Box::new(t)),
                        None => t,
                    });
                }
                sum
            }
            "measured_factor" | "coat_absorption_factor" => {
                self.warn(format!("{}: dropped", at(self)));
                self.bsdf_tree(&Self::df_arg(args, "base"), around)?
            }
            "measured_bsdf" => {
                self.warn(format!("{}: scatters nothing", at(self)));
                None
            }
            other => {
                self.warn(format!("df::{other} (line {line}): not a surface BSDF, scatters nothing"));
                None
            }
        })
    }

    /// A mix's component weights: as given (unbounded), divided by their sum where it passes one (normalized), or
    /// each limited to what the earlier ones left (clamped).
    fn mix_weights(&mut self, name: &str, weights: Vec<Num>) -> Result<Vec<Num>, Error> {
        let o = "df::mix";
        if weights.is_empty() || name.ends_with("unbounded_mix") {
            return Ok(weights);
        }
        let ty = self.num_type(&weights[0]);
        if name.ends_with("normalized_mix") {
            let mut sum = weights[0].clone();
            for w in &weights[1..] {
                sum = self.emit(Op::Add, &[sum, w.clone()], ty, o)?;
            }
            let at_least_one = self.emit(Op::Max, &[sum, Num::Known(Value::Float(1.0))], ty, o)?;
            return weights.iter().map(|w| self.emit(Op::Divide, &[w.clone(), at_least_one.clone()], ty, o)).collect();
        }
        let mut left = Num::Known(Value::splat(ty, 1.0));
        let mut out = Vec::new();
        for w in weights {
            let w = self.emit(Op::Min, &[w, left.clone()], ty, o)?;
            left = self.emit(Op::Subtract, &[left, w.clone()], ty, o)?;
            out.push(w);
        }
        Ok(out)
    }

    /// A float or color weight and a tint as the lobe's float weight and color tint.
    fn split_weight(&mut self, w: &Num, tint: &Num) -> Result<(Num, Num), Error> {
        if self.num_type(w) == Type::Float {
            return Ok((w.clone(), tint.clone()));
        }
        let t = self.mul(w, tint, Type::Color3, "df")?;
        Ok((Num::Known(Value::Float(1.0)), t))
    }

    fn as_color(&mut self, n: &Num) -> Result<Num, Error> {
        match self.num_type(n) {
            Type::Color3 => Ok(n.clone()),
            _ => self.emit(Op::Convert, std::slice::from_ref(n), Type::Color3, "df"),
        }
    }

    /// A color that should be gray as a float: its first channel, with a warning when the channels differ.
    fn gray(&mut self, n: &Num, at: &str, what: &str) -> Result<Num, Error> {
        if self.num_type(n) == Type::Float {
            return Ok(n.clone());
        }
        if let Some(v) = n.known() {
            let f = v.floats();
            if f.iter().any(|x| *x != f[0]) {
                self.warn(format!("{at}: a colored {what}, its average"));
            }
            return Ok(Num::Known(Value::Float(f.iter().sum::<f32>() / f.len() as f32)));
        }
        self.warn(format!("{at}: a colored {what}, its first channel"));
        self.emit(Op::Extract(0), std::slice::from_ref(n), Type::Float, "df")
    }

    /// The `vector2` roughness of MDL's two roughnesses (MDL's roughness is the microfacet alpha, as pbrlib's).
    fn roughness(&mut self, r: &[Num; 2]) -> Result<Arg, Error> {
        let n = self
            .from_parts(&Ty::float_vector(2), r.to_vec(), "df.roughness")
            .map_err(|m| self.bad(Default::default(), m))?;
        Ok(num_arg(n.num().expect("a vector")))
    }

    /// The emission tree of an `edf` value; `None` emits nothing.
    fn edf_tree(&mut self, df: &Df) -> Result<Option<Tree>, Error> {
        let (name, args, line) = match df {
            Df::Default => return Ok(None),
            Df::Select { cond, then, otherwise } => {
                let a = self.edf_tree(then)?;
                let b = self.edf_tree(otherwise)?;
                let w = self.emit(Op::Convert, &[Num::Node(*cond)], Type::Float, "df")?;
                return mix_trees(a, b, w, self);
            }
            Df::Call { name, args, line } => (name.as_str(), args.as_slice(), *line),
        };
        let white =
            || Tree::Lobe { kind: "uniform_edf", inputs: vec![("color".into(), Arg::Value(Value::Color3([1.0; 3])))] };
        Ok(match name {
            "diffuse_edf" => Some(white()),
            "spot_edf" | "measured_edf" => {
                self.warn(format!("df::{name} (line {line}): uniform emission"));
                Some(white())
            }
            "tint" => {
                let base = self.edf_tree(&Self::df_arg(args, "base"))?;
                let tint = Self::num_arg(args, "tint").unwrap_or(Num::Known(Value::Color3([1.0; 3])));
                base.map(|b| Tree::Multiply(Box::new(b), tint))
            }
            "directional_factor" => {
                self.warn(format!("df::directional_factor (line {line}) on emission: the normal tint"));
                let base = self.edf_tree(&Self::df_arg(args, "base"))?;
                let tint = Self::num_arg(args, "normal_tint").unwrap_or(Num::Known(Value::Color3([1.0; 3])));
                base.map(|b| Tree::Multiply(Box::new(b), tint))
            }
            "normalized_mix"
            | "clamped_mix"
            | "unbounded_mix"
            | "color_normalized_mix"
            | "color_clamped_mix"
            | "color_unbounded_mix" => {
                let components = Self::arg(args, "components").and_then(Val::elements).unwrap_or(&[]).to_vec();
                let mut parts = Vec::new();
                for c in &components {
                    let w =
                        self.field(c, "weight").and_then(|v| v.num().cloned()).unwrap_or(Num::Known(Value::Float(0.0)));
                    let d = self
                        .field(c, "component")
                        .and_then(|v| v.df_value().cloned())
                        .unwrap_or_else(|| Rc::new(Df::Default));
                    parts.push((w, d));
                }
                let weights = self.mix_weights(name, parts.iter().map(|(w, _)| w.clone()).collect())?;
                let mut sum: Option<Tree> = None;
                for ((_, d), w) in parts.iter().zip(weights) {
                    let Some(t) = self.edf_tree(d)? else { continue };
                    let t = weighted(t, w);
                    sum = Some(match sum {
                        Some(s) => Tree::Add(Box::new(s), Box::new(t)),
                        None => t,
                    });
                }
                sum
            }
            other => {
                self.warn(format!("df::{other} (line {line}): not an EDF, emits nothing"));
                None
            }
        })
    }
}

/// `n` as a node input.
fn num_arg(n: &Num) -> Arg {
    match n {
        Num::Known(v) => Arg::Value(v.clone()),
        Num::Node(id) => Arg::Node(*id),
    }
}

fn scatter_mode(mode: i32) -> &'static str {
    match mode {
        1 => "T",
        2 => "RT",
        _ => "R",
    }
}

fn lobe_geometry(inputs: &mut Vec<(String, Arg)>, tangent: Option<Num>, normal: Option<Num>) {
    if let Some(t) = tangent {
        inputs.push(("tangent".into(), num_arg(&t)));
    }
    if let Some(n) = normal {
        inputs.push(("normal".into(), num_arg(&n)));
    }
}

/// `t` scaled by `w`, unless `w` is a known one.
fn weighted(t: Tree, w: Num) -> Tree {
    match w.known() {
        Some(v) if v.floats().iter().all(|x| *x == 1.0) => t,
        _ => Tree::Multiply(Box::new(t), w),
    }
}

fn layered(top: Option<Tree>, base: Option<Tree>) -> Option<Tree> {
    match (top, base) {
        (Some(t), Some(b)) => Some(Tree::Layer(Box::new(t), Box::new(b))),
        (t, None) => t,
        (None, b) => b,
    }
}

/// `mix(fg = a, bg = b, mix = w)`, a missing side scattering nothing.
fn mix_trees(a: Option<Tree>, b: Option<Tree>, w: Num, ev: &mut Eval) -> Result<Option<Tree>, Error> {
    Ok(match (a, b) {
        (Some(a), Some(b)) => Some(Tree::Mix { fg: Box::new(a), bg: Box::new(b), weight: w }),
        (Some(a), None) => Some(Tree::Multiply(Box::new(a), w)),
        (None, Some(b)) => {
            let rest = ev.emit(Op::Subtract, &[Num::Known(Value::Float(1.0)), w], Type::Float, "df")?;
            Some(Tree::Multiply(Box::new(b), rest))
        }
        (None, None) => None,
    })
}

/// `t` with a thin film on its dielectric, conductor and Schlick lobes.
fn with_thin_film(t: Tree, thickness: &Num, ior: &Num) -> Tree {
    match t {
        Tree::Lobe { kind, mut inputs }
            if matches!(kind, "dielectric_bsdf" | "conductor_bsdf" | "generalized_schlick_bsdf") =>
        {
            inputs.retain(|(n, _)| n != "thinfilm_thickness" && n != "thinfilm_ior");
            inputs.push(("thinfilm_thickness".into(), num_arg(thickness)));
            inputs.push(("thinfilm_ior".into(), num_arg(ior)));
            Tree::Lobe { kind, inputs }
        }
        Tree::Layer(a, b) => {
            Tree::Layer(Box::new(with_thin_film(*a, thickness, ior)), Box::new(with_thin_film(*b, thickness, ior)))
        }
        Tree::Mix { fg, bg, weight } => Tree::Mix {
            fg: Box::new(with_thin_film(*fg, thickness, ior)),
            bg: Box::new(with_thin_film(*bg, thickness, ior)),
            weight,
        },
        Tree::Add(a, b) => {
            Tree::Add(Box::new(with_thin_film(*a, thickness, ior)), Box::new(with_thin_film(*b, thickness, ior)))
        }
        Tree::Multiply(a, w) => Tree::Multiply(Box::new(with_thin_film(*a, thickness, ior)), w),
        other => other,
    }
}

/// The shader-level nodes of a material: its trees lowered, then the `surface` node over them.
struct Nodes {
    nodes: Vec<ShaderNode>,
}

impl Nodes {
    fn push(
        &mut self,
        category: &str,
        closure: &str,
        inputs: Vec<(String, Arg)>,
        links: Vec<(String, String)>,
    ) -> String {
        let name = format!("{}_{}", category, self.nodes.len());
        self.nodes.push(ShaderNode {
            name: name.clone(),
            category: category.to_string(),
            port_type: PortType::Closure(closure.to_string()),
            inputs,
            links,
        });
        name
    }

    /// The tree as nodes of closure type `closure` (`BSDF`, `EDF`); the name of its root.
    fn emit(&mut self, ev: &mut Eval, t: Tree, closure: &str, ior: &Num) -> Result<String, Error> {
        Ok(match t {
            Tree::Lobe { kind, inputs } => self.push(kind, closure, inputs, Vec::new()),
            Tree::Specular { tint, roughness, tangent, mode, normal } => {
                let mut inputs = vec![("roughness".into(), ev.roughness(&roughness)?)];
                let kind = if mode == 0 {
                    inputs.push(("color0".into(), num_arg(&tint)));
                    inputs.push(("color90".into(), num_arg(&tint)));
                    inputs.push(("color82".into(), num_arg(&tint)));
                    "generalized_schlick_bsdf"
                } else {
                    // Transmission refracts by the material's ior.
                    inputs.push(("tint".into(), num_arg(&tint)));
                    inputs.push(("ior".into(), num_arg(ior)));
                    inputs.push(("scatter_mode".into(), Arg::Text(scatter_mode(mode).into())));
                    "dielectric_bsdf"
                };
                lobe_geometry(&mut inputs, tangent, normal);
                self.push(kind, closure, inputs, Vec::new())
            }
            Tree::Layer(top, base) => {
                let top = self.emit(ev, *top, closure, ior)?;
                let base = self.emit(ev, *base, closure, ior)?;
                self.push("layer", closure, Vec::new(), vec![("top".into(), top), ("base".into(), base)])
            }
            Tree::Mix { fg, bg, weight } => {
                let fg = self.emit(ev, *fg, closure, ior)?;
                let bg = self.emit(ev, *bg, closure, ior)?;
                self.push(
                    "mix",
                    closure,
                    vec![("mix".into(), num_arg(&weight))],
                    vec![("fg".into(), fg), ("bg".into(), bg)],
                )
            }
            Tree::Add(a, b) => {
                let a = self.emit(ev, *a, closure, ior)?;
                let b = self.emit(ev, *b, closure, ior)?;
                self.push("add", closure, Vec::new(), vec![("in1".into(), a), ("in2".into(), b)])
            }
            Tree::Multiply(a, w) => {
                let a = self.emit(ev, *a, closure, ior)?;
                self.push("multiply", closure, vec![("in2".into(), num_arg(&w))], vec![("in1".into(), a)])
            }
        })
    }
}

/// The material value's surface as `ShaderNode`s, and the name of the `surface` node.
fn finish(ev: &mut Eval, m: &Val) -> Result<(Vec<ShaderNode>, String), Error> {
    let field = |ev: &Eval, v: &Val, path: &[&str]| -> Option<Val> {
        let mut cur = v.clone();
        for p in path {
            cur = ev.field(&cur, p)?.clone();
        }
        Some(cur)
    };
    let df_at = |ev: &Eval, path: &[&str]| {
        field(ev, m, path).and_then(|v| v.df_value().cloned()).unwrap_or_else(|| Rc::new(Df::Default))
    };
    let ior = field(ev, m, &["ior"]).and_then(|v| v.num().cloned()).unwrap_or(Num::Known(Value::Color3([1.0; 3])));
    let ior = ev.gray(&ior, "material", "ior")?;
    let scattering = df_at(ev, &["surface", "scattering"]);
    let emission = df_at(ev, &["surface", "emission", "emission"]);
    // The geometry normal is every lobe's normal (MDL's lobes take theirs from it, or from a layer).
    let around = Around { normal: ev.normal.map(Num::Node), ior: ior.clone() };
    let bsdf = ev.bsdf_tree(&scattering, &around)?;
    let mut edf = ev.edf_tree(&emission)?;
    if let Some(e) = edf.take() {
        let intensity = field(ev, m, &["surface", "emission", "intensity"])
            .and_then(|v| v.num().cloned())
            .unwrap_or(Num::Known(Value::Color3([0.0; 3])));
        if field(ev, m, &["surface", "emission", "mode"]).and_then(|v| v.known_int()) == Some(1) {
            ev.warn("emission intensity_power read as radiant exitance");
        }
        // MaterialX's uniform EDF emits its color as radiance; MDL's intensity is the radiant exitance.
        let radiance = ev.emit(
            Op::Divide,
            &[intensity, Num::Known(Value::Float(std::f32::consts::PI))],
            Type::Color3,
            "emission",
        )?;
        if radiance.known() != Some(&Value::Color3([0.0; 3])) {
            edf = Some(match e {
                Tree::Lobe { kind: "uniform_edf", .. } => {
                    Tree::Lobe { kind: "uniform_edf", inputs: vec![("color".into(), num_arg(&radiance))] }
                }
                other => Tree::Multiply(Box::new(other), radiance),
            });
        }
    }
    for (path, what) in [
        (&["volume", "scattering"][..], "volume scattering"),
        (&["backface", "scattering"][..], "backface"),
        (&["hair"][..], "hair BSDF"),
    ] {
        if !matches!(*df_at(ev, path), Df::Default) {
            ev.warn(format!("{what} ignored"));
        }
    }
    for (path, what) in [
        (&["volume", "absorption_coefficient"][..], "volume absorption"),
        (&["volume", "scattering_coefficient"][..], "volume scattering coefficient"),
        (&["geometry", "displacement"][..], "displacement"),
    ] {
        if let Some(v) = field(ev, m, path)
            && v.known_value().is_none_or(|x| x.floats().iter().any(|c| *c != 0.0))
        {
            ev.warn(format!("{what} ignored"));
        }
    }
    let mut nodes = Nodes { nodes: Vec::new() };
    let mut links = Vec::new();
    if let Some(b) = bsdf {
        links.push(("bsdf".to_string(), nodes.emit(ev, b, "BSDF", &ior)?));
    }
    if let Some(e) = edf {
        links.push(("edf".to_string(), nodes.emit(ev, e, "EDF", &ior)?));
    }
    let mut inputs = Vec::new();
    if let Some(o) = field(ev, m, &["geometry", "cutout_opacity"]).and_then(|v| v.num().cloned())
        && o.known() != Some(&Value::Float(1.0))
    {
        inputs.push(("opacity".to_string(), num_arg(&o)));
    }
    if let Some(t) = field(ev, m, &["thin_walled"]).and_then(|v| v.num().cloned())
        && t.known() != Some(&Value::Boolean(false))
    {
        inputs.push(("thin_walled".to_string(), num_arg(&t)));
    }
    let surface = "surface".to_string();
    nodes.nodes.push(ShaderNode {
        name: surface.clone(),
        category: "surface".to_string(),
        port_type: PortType::SurfaceShader,
        inputs,
        links,
    });
    Ok((nodes.nodes, surface))
}
