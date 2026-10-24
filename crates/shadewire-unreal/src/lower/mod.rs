//! Lowering: a material's pins become IR through `shadewire-nodes`, one expression output at a time, back to the
//! values they read. Function calls open a frame of their own; reroutes, switches and function inputs are followed
//! as Unreal's compiler follows them, and each input's mask is applied to what it reads, as Unreal applies it.

pub mod fallback;
pub mod math;
pub mod params;
pub mod structure;
pub mod surface;
pub mod texture;
pub mod vector;

use std::collections::{HashMap, HashSet};

use shadewire::{Error, NodeId, SourceInfo, Type, Value};
use shadewire_nodes::elaborate::STACK_BYTES;
use shadewire_nodes::{Arg, Elaborator, IrOptions, MaterialIr, Synthetic};

use crate::document::{Document, Expression, Material, Texture, at};
use params::Params;

/// The class prefix every expression class shares.
const PREFIX: &str = "MaterialExpression";

/// One expression list being lowered: the material's, or a function's inlined for one call.
pub struct Frame<'a> {
    exprs: HashMap<&'a str, &'a Expression>,
    /// The frame of the call that opened this one, and the call.
    call: Option<(usize, &'a Expression)>,
    /// `"{call}/"` for each call on the way down, empty for the material.
    prefix: String,
}

/// The state of one material's lowering: the elaborator making the IR, the document, and what it made so far.
pub struct Lowerer<'a> {
    pub e: Elaborator<'a>,
    pub doc: &'a Document,
    /// The parameter values the material and its instance chain give.
    pub params: Params,
    frames: Vec<Frame<'a>>,
    /// `(frame, call)` to the frame of its function.
    calls: HashMap<(usize, String), usize>,
    /// `(frame, expression, output)` to the IR node it lowered to.
    memo: HashMap<(usize, String, usize), NodeId>,
    in_progress: HashSet<(usize, String, usize)>,
    /// Parameter name to its uniform, so every expression naming a parameter reads one.
    pub uniforms: HashMap<String, NodeId>,
    pub warnings: Vec<String>,
}

/// The vector type with `n` components, a float for one.
pub fn vector_type(n: usize) -> Type {
    match n {
        1 => Type::Float,
        2 => Type::Vector2,
        3 => Type::Vector3,
        _ => Type::Vector4,
    }
}

/// The short class name, without `MaterialExpression`.
pub fn short(class: &str) -> &str {
    class.strip_prefix(PREFIX).unwrap_or(class)
}

impl<'a> Lowerer<'a> {
    pub fn new(e: Elaborator<'a>, doc: &'a Document, base: &'a Material, params: Params) -> Lowerer<'a> {
        let frame = Frame {
            exprs: base.expressions.iter().map(|x| (x.name.as_str(), x)).collect(),
            call: None,
            prefix: String::new(),
        };
        Lowerer {
            e,
            doc,
            params,
            frames: vec![frame],
            calls: HashMap::new(),
            memo: HashMap::new(),
            in_progress: HashSet::new(),
            uniforms: HashMap::new(),
            warnings: Vec::new(),
        }
    }

    pub fn expression(&self, frame: usize, name: &str) -> Option<&'a Expression> {
        self.frames[frame].exprs.get(name).copied()
    }

    /// `{prefix}{name}`: the expression's name with the calls that inlined it.
    pub fn origin(&self, frame: usize, x: &Expression) -> String {
        format!("{}{}", self.frames[frame].prefix, x.name)
    }

    /// Records `{class} "{prefix}{name}": {what}`, once.
    pub fn warn(&mut self, frame: usize, x: &Expression, what: impl Into<String>) {
        let warning = format!("{} \"{}\": {}", short(&x.class), self.origin(frame, x), what.into());
        if !self.warnings.contains(&warning) {
            self.warnings.push(warning);
        }
    }

    /// Output `index` of the expression `name` in `frame`, lowered once.
    pub fn output(&mut self, frame: usize, name: &str, index: usize) -> Result<NodeId, Error> {
        let key = (frame, name.to_string(), index);
        if let Some(&id) = self.memo.get(&key) {
            return Ok(id);
        }
        let Some(x) = self.expression(frame, name) else {
            return Err(Error::BadValue { at: at(name), message: format!("no expression named {name}") });
        };
        if !self.in_progress.insert(key.clone()) {
            return Err(Error::Cycle { at: at(name), node: name.to_string() });
        }
        let lowered = self.dispatch(frame, x, index);
        self.in_progress.remove(&key);
        let id = lowered?;
        self.memo.insert(key, id);
        Ok(id)
    }

    fn dispatch(&mut self, f: usize, x: &'a Expression, out: usize) -> Result<NodeId, Error> {
        match short(&x.class) {
            "Constant" | "Constant2Vector" | "Constant3Vector" | "Constant4Vector" => vector::constant(self, f, x),
            "ScalarParameter" | "VectorParameter" => params::parameter(self, f, x),
            "StaticBool" | "StaticBoolParameter" => params::static_bool_value(self, f, x),
            "Reroute" => structure::reroute(self, f, x),
            "NamedRerouteDeclaration" => structure::reroute(self, f, x),
            "NamedRerouteUsage" => structure::named_reroute(self, f, x),
            "StaticSwitch" | "StaticSwitchParameter" => structure::static_switch(self, f, x),
            "QualitySwitch" | "FeatureLevelSwitch" | "ShadingPathSwitch" => structure::engine_switch(self, f, x),
            "MaterialFunctionCall" => structure::function_call(self, f, x, out),
            "FunctionInput" => structure::function_input(self, f, x),
            "FunctionOutput" => structure::function_output(self, f, x),
            "Add" => math::binary(self, f, x, "add"),
            "Subtract" => math::binary(self, f, x, "subtract"),
            "Multiply" => math::binary(self, f, x, "multiply"),
            "Divide" => math::binary(self, f, x, "divide"),
            "Min" => math::binary(self, f, x, "min"),
            "Max" => math::binary(self, f, x, "max"),
            "Fmod" => math::fmod(self, f, x),
            "Power" => math::power(self, f, x),
            "OneMinus" => math::one_minus(self, f, x),
            "Abs" => math::unary(self, f, x, "absval"),
            "Floor" => math::unary(self, f, x, "floor"),
            "Ceil" => math::unary(self, f, x, "ceil"),
            "Round" => math::unary(self, f, x, "round"),
            "Frac" => math::unary(self, f, x, "fract"),
            "Sign" => math::unary(self, f, x, "sign"),
            "SquareRoot" => math::unary(self, f, x, "sqrt"),
            "Exponential" => math::unary(self, f, x, "exp"),
            "Truncate" => math::truncate(self, f, x),
            "Saturate" => math::saturate(self, f, x),
            "Clamp" => math::clamp(self, f, x),
            "LinearInterpolate" => math::lerp(self, f, x),
            "Sine" => math::periodic(self, f, x, "sin"),
            "Cosine" => math::periodic(self, f, x, "cos"),
            "Tangent" => math::periodic(self, f, x, "tan"),
            "Arcsine" | "ArcsineFast" => math::unary(self, f, x, "asin"),
            "Arccosine" | "ArccosineFast" => math::unary(self, f, x, "acos"),
            "Arctangent2" | "Arctangent2Fast" => math::atan2(self, f, x),
            "If" => math::if_compare(self, f, x),
            "Step" => math::step(self, f, x),
            "SmoothStep" => math::smooth_step(self, f, x),
            "AppendVector" => vector::append(self, f, x),
            "ComponentMask" => vector::component_mask(self, f, x),
            "DotProduct" => vector::dot(self, f, x),
            "CrossProduct" => vector::cross(self, f, x),
            "Normalize" => vector::normalize(self, f, x),
            "Length" => vector::length(self, f, x),
            "Distance" => vector::distance(self, f, x),
            "Desaturation" => vector::desaturation(self, f, x),
            "TextureSample" | "TextureSampleParameter2D" => texture::sample(self, f, x),
            "TextureCoordinate" => texture::texture_coordinate(self, f, x),
            "Panner" => texture::panner(self, f, x),
            "Time" => texture::time(self, f, x),
            "WorldPosition" => texture::world_position(self, f, x),
            "VertexNormalWS" => texture::vertex_normal(self, f, x),
            "PixelNormalWS" => texture::pixel_normal(self, f, x),
            "CameraVectorWS" => texture::camera_vector(self, f, x),
            "VertexColor" => texture::vertex_color(self, f, x),
            "Fresnel" => texture::fresnel(self, f, x),
            _ => fallback::neutral(self, f, x),
        }
    }

    /// What `link` reads in `frame`: its source output with `mask` applied.
    pub fn link(&mut self, frame: usize, from: &(String, usize), mask: Option<[u8; 4]>) -> Result<NodeId, Error> {
        let id = self.output(frame, &from.0, from.1)?;
        match mask {
            Some(mask) => self.mask(id, mask, &from.0),
            None => Ok(id),
        }
    }

    /// The input `name` of `x` when something is linked to it.
    pub fn input(&mut self, f: usize, x: &Expression, name: &str) -> Result<Option<NodeId>, Error> {
        match x.input(name) {
            Some(input) => match &input.from {
                Some(from) => self.link(f, from, input.mask).map(Some),
                None => Ok(None),
            },
            None => Ok(None),
        }
    }

    /// The input at `index` of `x` when something is linked to it.
    pub fn input_at(&mut self, f: usize, x: &Expression, index: usize) -> Result<Option<NodeId>, Error> {
        match x.input_at(index) {
            Some(input) => match &input.from {
                Some(from) => self.link(f, from, input.mask).map(Some),
                None => Ok(None),
            },
            None => Ok(None),
        }
    }

    /// The input `name` of `x`, or the float property `constant` (or `default` when the property is absent) when
    /// nothing is linked to it.
    pub fn input_or(
        &mut self,
        f: usize,
        x: &Expression,
        name: &str,
        constant: &str,
        default: f32,
    ) -> Result<NodeId, Error> {
        match self.input(f, x, name)? {
            Some(id) => Ok(id),
            None => {
                let value = x.number_or(constant, default);
                Ok(self.k(value, &self.origin(f, x)))
            }
        }
    }

    /// The input `name` of `x`, or zero with a warning when nothing is linked to it (Unreal fails to compile it).
    pub fn required(&mut self, f: usize, x: &Expression, name: &str) -> Result<NodeId, Error> {
        match self.input(f, x, name)? {
            Some(id) => Ok(id),
            None => {
                self.warn(f, x, format!("{name} is not linked, read as 0"));
                Ok(self.k(0.0, &self.origin(f, x)))
            }
        }
    }

    /// The components of `id` that `mask` selects, in order.
    pub fn mask(&mut self, id: NodeId, mask: [u8; 4], origin: &str) -> Result<NodeId, Error> {
        let n = self.dims(id);
        let picked: Vec<u32> = (0..4u32).filter(|&i| mask[i as usize] != 0).collect();
        if picked.is_empty() || picked.iter().copied().eq(0..n as u32) {
            return Ok(id);
        }
        if let Some(&beyond) = picked.iter().find(|&&i| i as usize >= n) {
            return Err(Error::BadValue {
                at: at(origin),
                message: format!("mask reads component {beyond} of a {}-component value", n),
            });
        }
        // A leading run of two or more components is a truncation, which `convert` makes in one node.
        if picked.len() >= 2 && picked.iter().copied().eq(0..picked.len() as u32) {
            let id = self.plain(id, origin)?;
            return self.op("convert", vector_type(picked.len()), &[("in", Arg::Node(id))], origin);
        }
        let channels: Vec<NodeId> = picked.iter().map(|&i| self.channel(id, i, origin)).collect::<Result<_, _>>()?;
        self.combine(&channels, origin)
    }

    /// Component `index` of `id`, the value itself for a float.
    pub fn channel(&mut self, id: NodeId, index: u32, origin: &str) -> Result<NodeId, Error> {
        if self.dims(id) == 1 {
            return Ok(id);
        }
        let index = Arg::Value(Value::Integer(index as i32));
        self.op("extract", Type::Float, &[("in", Arg::Node(id)), ("index", index)], origin)
    }

    /// A vector of the floats `channels` (the float itself when there is one).
    pub fn combine(&mut self, channels: &[NodeId], origin: &str) -> Result<NodeId, Error> {
        let category = match channels.len() {
            1 => return Ok(channels[0]),
            2 => "combine2",
            3 => "combine3",
            4 => "combine4",
            n => return Err(Error::BadValue { at: at(origin), message: format!("a vector of {n} components") }),
        };
        let names = ["in1", "in2", "in3", "in4"];
        let inputs: Vec<(&str, Arg)> = names.iter().zip(channels).map(|(n, &id)| (*n, Arg::Node(id))).collect();
        self.op(category, vector_type(channels.len()), &inputs, origin)
    }

    /// How many components `id` has (a color counts as the vector of its width).
    pub fn dims(&self, id: NodeId) -> usize {
        self.e.dag.node(id).ty.component_count()
    }

    /// `id` as a vector of `n` components: a float splats, a wider vector is truncated, a narrower one is padded
    /// with zeros, as Unreal's casts do.
    pub fn cast(&mut self, id: NodeId, n: usize, origin: &str) -> Result<NodeId, Error> {
        let ty = self.e.dag.node(id).ty;
        let have = ty.component_count();
        if matches!(ty, Type::Color3 | Type::Color4) {
            let id = self.op("convert", vector_type(have), &[("in", Arg::Node(id))], origin)?;
            return self.cast(id, n, origin);
        }
        if have == n {
            return Ok(id);
        }
        if have == 1 {
            return self.op("convert", vector_type(n), &[("in", Arg::Node(id))], origin);
        }
        let mut channels = Vec::with_capacity(n);
        for i in 0..n {
            channels.push(if i < have { self.channel(id, i as u32, origin)? } else { self.k(0.0, origin) });
        }
        self.combine(&channels, origin)
    }

    /// `id` as the float or vector of its width: a color becomes the vector of its components.
    pub fn plain(&mut self, id: NodeId, origin: &str) -> Result<NodeId, Error> {
        let n = self.dims(id);
        self.cast(id, n, origin)
    }

    /// `id` as a `color3`.
    pub fn color3(&mut self, id: NodeId, origin: &str) -> Result<NodeId, Error> {
        let v = self.cast(id, 3, origin)?;
        self.op("convert", Type::Color3, &[("in", Arg::Node(v))], origin)
    }

    /// Output `out` of a `category` node over `inputs`, through the elaborator.
    pub fn op(&mut self, category: &str, out: Type, inputs: &[(&str, Arg)], origin: &str) -> Result<NodeId, Error> {
        self.e.instantiate(category, out, inputs, origin)
    }

    pub fn op1(&mut self, category: &str, a: NodeId, origin: &str) -> Result<NodeId, Error> {
        let ty = self.e.dag.node(a).ty;
        self.op(category, ty, &[("in", Arg::Node(a))], origin)
    }

    /// The two-input `category` node over `a` and `b`, a float operand splatted to the other's width.
    pub fn op2(&mut self, category: &str, a: NodeId, b: NodeId, origin: &str) -> Result<NodeId, Error> {
        let (a, b) = self.widen(a, b, origin)?;
        let ty = self.e.dag.node(a).ty;
        self.op(category, ty, &[("in1", Arg::Node(a)), ("in2", Arg::Node(b))], origin)
    }

    /// `a` and `b` at one width: a float splats to the other's; two vectors of different widths are an error,
    /// as in Unreal.
    pub fn widen(&mut self, a: NodeId, b: NodeId, origin: &str) -> Result<(NodeId, NodeId), Error> {
        let (a, b) = (self.plain(a, origin)?, self.plain(b, origin)?);
        let (na, nb) = (self.dims(a), self.dims(b));
        match (na, nb) {
            _ if na == nb => Ok((a, b)),
            (1, n) => Ok((self.cast(a, n, origin)?, b)),
            (n, 1) => Ok((a, self.cast(b, n, origin)?)),
            _ => Err(Error::TypeMismatch {
                at: at(origin),
                expected: vector_type(na).mtlx_name().to_string(),
                found: vector_type(nb).mtlx_name().to_string(),
            }),
        }
    }

    /// A float constant the lowering introduces.
    pub fn k(&mut self, x: f32, origin: &str) -> NodeId {
        self.e.constant(Value::Float(x), origin)
    }

    /// A vector constant of `values`' width.
    pub fn kv(&mut self, values: &[f32], origin: &str) -> NodeId {
        let value = match values {
            [x] => Value::Float(*x),
            [x, y] => Value::Vector2([*x, *y]),
            [x, y, z] => Value::Vector3([*x, *y, *z]),
            [x, y, z, w, ..] => Value::Vector4([*x, *y, *z, *w]),
            [] => Value::Float(0.0),
        };
        self.e.constant(value, origin)
    }

    /// A zero of `n` components.
    pub fn zero(&mut self, n: usize, origin: &str) -> NodeId {
        self.kv(&vec![0.0; n], origin)
    }

    /// The frame of `call`'s function, opened once per call.
    pub fn call_frame(&mut self, f: usize, call: &'a Expression) -> Result<Option<usize>, Error> {
        let key = (f, call.name.clone());
        if let Some(&frame) = self.calls.get(&key) {
            return Ok(Some(frame));
        }
        let path = call.text("MaterialFunction").unwrap_or("");
        let Some(function) = self.doc.functions.iter().find(|func| func.path == path) else {
            self.warn(f, call, format!("function {path} is not in the document"));
            return Ok(None);
        };
        let prefix = format!("{}{}/", self.frames[f].prefix, call.name);
        let exprs = function.expressions.iter().map(|x| (x.name.as_str(), x)).collect();
        self.frames.push(Frame { exprs, call: Some((f, call)), prefix });
        let frame = self.frames.len() - 1;
        self.calls.insert(key, frame);
        Ok(Some(frame))
    }

    /// The call that opened `frame`, with the caller's frame.
    pub fn caller(&self, frame: usize) -> Option<(usize, &'a Expression)> {
        self.frames[frame].call
    }

    /// `frame`'s expressions of `class`, in Unreal's order: by `SortPriority`, then as listed.
    pub fn sorted(&self, class: &str, list: &'a [Expression]) -> Vec<&'a Expression> {
        let mut found: Vec<&Expression> = list.iter().filter(|x| short(&x.class) == class).collect();
        found.sort_by_key(|x| x.number("SortPriority").map(|p| p as i64).unwrap_or(0));
        found
    }

    /// The expressions of the function `frame` was opened for.
    pub fn function_expressions(&self, frame: usize) -> Option<&'a [Expression]> {
        let (_, call) = self.frames[frame].call?;
        let path = call.text("MaterialFunction")?;
        self.doc.functions.iter().find(|func| func.path == path).map(|func| func.expressions.as_slice())
    }

    /// The document's texture at `path`.
    pub fn texture(&self, path: &str) -> Option<&'a Texture> {
        self.doc.textures.iter().find(|t| t.path == path)
    }
}

/// `material` of `doc` lowered for `options`, on a thread with `STACK_BYTES` of stack (the caller's thread when
/// none can be made): lowering recurses along the graph.
pub fn lower(doc: &Document, material: &Material, options: &IrOptions) -> Result<MaterialIr, Error> {
    std::thread::scope(|threads| {
        let builder = std::thread::Builder::new().name("shadewire-unreal".to_string()).stack_size(STACK_BYTES);
        match builder.spawn_scoped(threads, || lower_here(doc, material, options)) {
            Ok(handle) => match handle.join() {
                Ok(result) => result,
                Err(panic) => std::panic::resume_unwind(panic),
            },
            Err(_) => lower_here(doc, material, options),
        }
    })
}

fn lower_here(doc: &Document, material: &Material, options: &IrOptions) -> Result<MaterialIr, Error> {
    let (base, params, mut warnings) = params::resolve(doc, material)?;
    let synthetic = Synthetic::new();
    let e = Elaborator::standalone(&synthetic, options);
    let l = Lowerer::new(e, doc, base, params);
    let (dag, mut lowered) = surface::finish(l, base)?;
    warnings.append(&mut lowered);
    let source = SourceInfo { source: "unreal".to_string(), mtlx_version: doc.producer.clone() };
    Ok(MaterialIr { dag, source, warnings })
}
