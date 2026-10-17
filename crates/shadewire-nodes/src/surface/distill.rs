//! Distilling a closure tree to `open_pbr_surface`, for hosts that light a fixed model and cannot walk lobes.
//!
//! The NVIDIA MDL SDK's distiller (its `ue4` and `transmissive_pbr` targets) is the reference for the semantics: mixed
//! and layered BSDFs collapse into one PBR parameter set, weighted by the mix weights, layering approximated. Every
//! distilled field is an IR expression over the tree's fields (lobe inputs, mix weights, multipliers), so uniforms
//! and textures stay live; known values fold.
//!
//! # Weights
//!
//! Each node of the tree receives a *flow*: a scalar weight `s` (the share of the surface it covers) and, when a
//! color multiplier lies above it, a tinted weight `t` (`Σ s·tint` over the paths reaching it). Weights are linear,
//! so a node shared by several parents sums what each sends it, and the tree is walked once, parents before
//! children (`ClosureGraph` stores children first):
//!
//! - the roots get `s = 1`;
//! - `Mix(fg, bg, m)` sends `s·m` to `fg` and `s·(1 − m)` to `bg`, `m` clamped to [0, 1];
//! - `Add(a, b)` sends `s` to both; the sum may pass one, which the normalisation below absorbs;
//! - `Multiply(c, k)` sends `s·k` for a float `k`; a color `k` keeps `s` and tints the colors under it (`t·k`);
//! - `Layer(top, base)` sends `s` to `top`. What reaches `base` depends on the top. open_pbr_surface layers its
//!   specular, coat and fuzz over its base itself, so a top of those lobes (a reflecting dielectric, a Schlick lobe
//!   with a Fresnel ramp, a sheen) leaves the base its full `s`: darkening it here would count the layering twice. A
//!   top of lobes open_pbr_surface does not layer hides what it covers: the base gets `s·(1 − top_weight·
//!   fresnel_avg)`, the top's cover being those lobes' weights combined as the flow is. A diffuse, a metal or a
//!   subsurface is opaque (its Fresnel average one); a transmitting lobe hides the base too, as MDL's layers count
//!   the light it transmits in its albedo (OmniSurface's `weighted_layer` of a transmission over its base);
//! - an `Empty` side scatters nothing: its share stays black.
//!
//! A lobe's own `weight` input multiplies its flow into its *coverage* `e = s·weight`.
//!
//! # Families
//!
//! | Lobe | Family | Fields |
//! |---|---|---|
//! | `oren_nayar_diffuse_bsdf`, `burley_diffuse_bsdf` | diffuse | `base_weight`, `base_color`, `base_diffuse_roughness` |
//! | `subsurface_bsdf`, `translucent_bsdf` | subsurface | `subsurface_weight`, `subsurface_color`, `subsurface_radius` and `subsurface_radius_scale` (the radius's brightest channel and the radius over it), `subsurface_scatter_anisotropy` |
//! | `dielectric_bsdf` reflecting (`R`, `RT`) | specular, or coat | `specular_weight`, `specular_color`, `specular_ior`, `specular_roughness`, `specular_roughness_anisotropy`; as a coat `coat_weight`, `coat_color`, `coat_ior`, `coat_roughness`, `coat_roughness_anisotropy` |
//! | `dielectric_bsdf` transmitting (`T`, `RT`) | transmission | `transmission_weight`, `transmission_color`; its ior and roughness join the specular's |
//! | `conductor_bsdf` | metal | `base_metalness`; `base_color` its normal-incidence reflectivity `((n−1)² + k²) / ((n+1)² + k²)` times its weight; `specular_roughness` |
//! | `generalized_schlick_bsdf` | metal; specular or coat when it is a Fresnel ramp in a layer's top | metal: `base_color` from `color0`, `specular_color` from `color82` unless it is `color0` (weighted by the metal's flow); specular: `specular_ior` from the reflectance `color0 / color90`, `specular_color` from `color90` |
//! | `sheen_bsdf` | fuzz | `fuzz_weight`, `fuzz_color`, `fuzz_roughness` |
//! | `thinfilm_thickness` > 0 on a specular or metal lobe | thin film | `thin_film_weight` (the share of those lobes with a film), `thin_film_thickness` (µm), `thin_film_ior` |
//! | `uniform_edf`; `conical_edf`, `measured_edf` with a warning | emission | `emission_luminance` (the radiance's brightest channel), `emission_color` (the radiance over it) |
//! | `generalized_schlick_edf` | its base's, tinted by `color0` | |
//! | the surface's `opacity` and `thin_walled`; the lobes' `normal` and `tangent` | geometry | `geometry_opacity`, `geometry_thin_walled`, `geometry_normal`, `geometry_tangent` (the first lobe's, a warning when lobes differ); a coat's `geometry_coat_normal`, `geometry_coat_tangent` |
//! | anything else (`chiang_hair_bsdf`, VDFs, unknown lobes) | none | dropped, with a warning naming it |
//! | a lobe input named `*dispersion*` (none in the vendored pbrlib) | none | dropped, with a warning naming dispersion |
//!
//! A parameter is the coverage-weighted average of its family's lobes: exact when the family has one lobe, or when
//! every lobe reads the same value. Where a family's total weight is zero the average reads the nodedef's default
//! instead (every division is guarded), and a lobe of zero weight is left out of the sum, not multiplied by zero, so
//! a NaN it computes there does not leak. Roughness is averaged as the microfacet alpha pair and inverted through
//! `open_pbr_anisotropy` into a roughness and an anisotropy. Fields no lobe sets keep the nodedef's defaults, except
//! that a tree with no specular, metal or transmission gets `specular_weight` zero.
//!
//! # Metal, dielectric and diffuse
//!
//! open_pbr_surface's base is `mix(metal, dielectric, base_metalness)`, the dielectric `mix(transmission, opaque,
//! transmission_weight)`, the opaque `mix(subsurface, diffuse, subsurface_weight)`, the diffuse at `base_weight`.
//! With `D`, `SS`, `T` the diffuse, subsurface and transmission coverages and `M` the metals' flow (a metal's own
//! weight scales its reflectivity into `base_color`), `C = D + SS + T + M`, the total `N = max(C, 1)` (the per-family
//! normalisation: shares stay in [0, 1] when lobes add past one) and the black share `K = N − C`:
//!
//! - `base_metalness = M / N`, `transmission_weight = T / (N − M)`, `subsurface_weight = SS / (N − M − T)`;
//! - `base_weight = D / (D + K)`: what the tree leaves black darkens the diffuse;
//! - `base_color` averages the diffuse colors (by coverage) and the metals' reflectivities (by flow).
//!
//! These invert open_pbr_surface's own tree exactly. `specular_weight` scales open_pbr_surface's metal as well as its
//! dielectric F0, so when the tree has a metal `specular_weight` is one and the specular coverage `w` (over the
//! dielectric share `N − M`) scales the dielectric's F0 into `specular_ior` instead, as open_pbr_surface itself does
//! with `specular_weight`; without a dielectric reflection that ior is one: no reflection. Without a metal,
//! `specular_weight` is `w`.
//!
//! # Coat
//!
//! A layer whose top reflects as a dielectric (a reflecting `dielectric_bsdf`, or a `generalized_schlick_bsdf` whose
//! `color0` and `color90` differ: a Fresnel ramp, as MDL's `custom_curve_layer` builds) is open_pbr_surface's
//! specular layer, unless it transmits nothing and its base already has such a reflection or is bare metal: then it
//! is the coat. A Schlick
//! lobe without a ramp (`color0 = color90`: MDL's tinted glossy lobe, its `directional_factor`) is a metal wherever
//! it is. `coat_weight` is the coat's coverage clamped to one; `coat_color` is the coat lobe's tint. Distilling
//! open_pbr_surface's own tree does not undo its darkening and tinting of the base under a coat.
//!
//! # Transmission and thin walls
//!
//! A transmitting lobe's coverage is its share of the dielectric (`transmission_weight`); its tint is
//! `transmission_color`; its ior and roughness average with the specular's (open_pbr_surface has one dielectric
//! interface). A medium under it (a VDF) is dropped with a warning, so `transmission_depth` and
//! `transmission_scatter` keep their defaults. A thin-walled surface stays thin-walled: open_pbr_surface then reads
//! transmission as thin and subsurface as diffuse transmission, which is what `translucent_bsdf` maps to.

use std::collections::{BTreeMap, HashMap};

use shadewire::interpret::ContextSource;
use shadewire::{
    ClosureGraph, ClosureId, ClosureNode, ClosureType, ContextRead, Dag, Error, Location, Node, NodeId, NodeKind, Op,
    Space, SurfaceField, TransformKind, Type, Value, eval_op,
};

use super::models::{CLOSURE_MODEL, fields};
use crate::document::empty_document;
use crate::elaborate::defaults::geomprop_read;
use crate::index::Index;
use crate::library::standard;

/// The model a closure tree is distilled to.
pub const DISTILL_MODEL: &str = "open_pbr_surface";

const NODEDEF: &str = "ND_open_pbr_surface_surfaceshader";
const EPSILON: f32 = 1e-6;
const ORIGIN: &str = "distill";

/// How every distiller note in `dag.approximations` (and `MaterialIr.warnings`) begins.
pub const NOTE_PREFIX: &str = "distill: ";

/// Replaces the closure tree of a `surface`-model dag by the `open_pbr_surface` fields it distills to (see the
/// module docs), adding what it drops or approximates to `dag.approximations`. A dag of any other model is left as
/// it is.
pub fn distill(dag: &mut Dag) -> Result<(), Error> {
    if dag.surface.model != CLOSURE_MODEL {
        return Ok(());
    }
    let Some(graph) = dag.surface.closure.take() else { return Ok(()) };
    let index = Index::new(empty_document(), standard());
    let at = || Location { file: None, line: 0, path: ORIGIN.to_string(), source: None };
    let nodedef =
        index.nodedef(NODEDEF).ok_or_else(|| Error::BadValue { at: at(), message: format!("no nodedef {NODEDEF}") })?;
    let field_nodes = dag.surface.fields.iter().map(|f| (f.name.clone(), f.node)).collect();
    let mut d = Distiller { dag, graph: &graph, field_nodes, notes: Vec::new(), reflects: HashMap::new() };
    let mut out = d.distill_fields();
    let mut distilled = Vec::new();
    for (name, ty) in fields(nodedef) {
        let node = match out.remove(&name) {
            Some(id) if d.ty(id) == ty => id,
            Some(id) => d.op(Op::Convert, &[id], ty),
            None => {
                let port = nodedef.inputs.iter().find(|p| p.name == name).expect("a field is a nodedef input");
                let origin = format!("{ORIGIN}.{name}");
                match (&port.default_geomprop, port.value.as_deref()) {
                    (Some(prop), _) => {
                        let read =
                            geomprop_read(&index, prop, ty).map_err(|message| Error::BadValue { at: at(), message })?;
                        let read_ty = read.ty();
                        d.dag.add(Node { kind: NodeKind::Context(read), ty: read_ty, origin })
                    }
                    (None, Some(text)) if !text.trim().is_empty() => {
                        let v =
                            Value::parse(ty, text).map_err(|e| Error::BadValue { at: at(), message: e.to_string() })?;
                        d.dag.add(Node { kind: NodeKind::Const(v), ty, origin })
                    }
                    _ => d.dag.add(Node { kind: NodeKind::Const(Value::zero(ty)), ty, origin }),
                }
            }
        };
        distilled.push(SurfaceField { name, ty, node });
    }
    let notes = std::mem::take(&mut d.notes);
    dag.surface.fields = distilled;
    dag.surface.model = DISTILL_MODEL.to_string();
    for note in notes {
        if !dag.approximations.contains(&note) {
            dag.approximations.push(note);
        }
    }
    Ok(())
}

/// Where a lobe sits: in no dielectric layer's top, or in the top of the specular layer or of the coat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Role {
    Base,
    Spec,
    Coat,
}

const ROLES: [Role; 3] = [Role::Base, Role::Spec, Role::Coat];

/// What reaches a node: its share `s` of the surface and, under a color multiplier, `t = Σ s·tint`.
#[derive(Clone, Copy, Debug)]
struct Flow {
    s: NodeId,
    t: Option<NodeId>,
}

/// One term of a weighted average: weight `w`, and `num` (the tinted `w·value`) when it is not simply `w·value`.
struct Item {
    w: NodeId,
    num: Option<NodeId>,
    value: NodeId,
}

#[derive(Default)]
struct Avg {
    items: Vec<Item>,
}

impl Avg {
    fn push(&mut self, w: NodeId, value: NodeId) {
        self.items.push(Item { w, num: None, value });
    }
}

/// A lobe's class, from its MaterialX category.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Class {
    Diffuse,
    Subsurface,
    Translucent,
    Dielectric,
    Conductor,
    Schlick,
    Sheen,
    Emission,
    SchlickEdf,
    Other,
}

fn class(kind: &str) -> Class {
    match kind {
        "oren_nayar_diffuse_bsdf" | "burley_diffuse_bsdf" => Class::Diffuse,
        "subsurface_bsdf" => Class::Subsurface,
        "translucent_bsdf" => Class::Translucent,
        "dielectric_bsdf" => Class::Dielectric,
        "conductor_bsdf" => Class::Conductor,
        "generalized_schlick_bsdf" => Class::Schlick,
        "sheen_bsdf" => Class::Sheen,
        "uniform_edf" | "conical_edf" | "measured_edf" => Class::Emission,
        "generalized_schlick_edf" => Class::SchlickEdf,
        _ => Class::Other,
    }
}

/// Whether a lobe's `scatter_mode` reflects and transmits (`R` when not set).
fn scatter(settings: &BTreeMap<String, String>) -> (bool, bool) {
    match settings.get("scatter_mode").map(|m| m.trim()) {
        Some("T") => (false, true),
        Some("RT") => (true, true),
        _ => (true, false),
    }
}

/// A host for folding constants: no reads, identity transforms.
struct NoContext;

impl ContextSource for NoContext {
    fn read(&mut self, read: &ContextRead) -> Value {
        Value::zero(read.ty())
    }
    fn transform(&mut self, _: TransformKind, _: Space, _: Space, v: [f32; 3]) -> [f32; 3] {
        v
    }
}

/// What the families accumulate.
#[derive(Default)]
struct Families {
    /// Coverages of the diffuse, subsurface and transmission lobes; flows of the metals.
    diffuse: Vec<NodeId>,
    subsurface: Vec<NodeId>,
    transmission: Vec<NodeId>,
    metal: Vec<NodeId>,
    base_color: Avg,
    diffuse_roughness: Avg,
    specular: Vec<NodeId>,
    specular_color: Avg,
    ior: Avg,
    alpha: Avg,
    transmission_color: Avg,
    subsurface_color: Avg,
    subsurface_radius: Avg,
    subsurface_anisotropy: Avg,
    coat: Vec<NodeId>,
    coat_color: Avg,
    coat_ior: Avg,
    coat_alpha: Avg,
    fuzz: Vec<NodeId>,
    fuzz_color: Avg,
    fuzz_roughness: Avg,
    /// `(weight, thickness in nm, ior)` of the specular and metal lobes.
    thin_film: Vec<(NodeId, NodeId, NodeId)>,
    emission: Vec<NodeId>,
    normals: Vec<NodeId>,
    tangents: Vec<NodeId>,
    coat_normals: Vec<NodeId>,
    coat_tangents: Vec<NodeId>,
}

struct Distiller<'d, 'g> {
    dag: &'d mut Dag,
    graph: &'g ClosureGraph,
    field_nodes: HashMap<String, NodeId>,
    notes: Vec<String>,
    reflects: HashMap<(ClosureId, bool), bool>,
}

impl Distiller<'_, '_> {
    // ---- IR, folding what is known ----

    fn ty(&self, id: NodeId) -> Type {
        self.dag.node(id).ty
    }

    fn known(&self, id: NodeId) -> Option<&Value> {
        match &self.dag.node(id).kind {
            NodeKind::Const(v) => Some(v),
            _ => None,
        }
    }

    fn is_splat(&self, id: NodeId, x: f32) -> bool {
        self.known(id).is_some_and(|v| v.floats().iter().all(|c| *c == x))
    }

    fn konst(&mut self, v: Value) -> NodeId {
        let ty = v.ty();
        self.dag.add(Node { kind: NodeKind::Const(v), ty, origin: ORIGIN.to_string() })
    }

    fn float(&mut self, x: f32) -> NodeId {
        self.konst(Value::Float(x))
    }

    fn op(&mut self, op: Op, args: &[NodeId], ty: Type) -> NodeId {
        let values: Option<Vec<Value>> = args.iter().map(|a| self.known(*a).cloned()).collect();
        if let Some(values) = values {
            let v = eval_op(&op, &values, ty, &mut NoContext);
            if v.ty() == ty {
                return self.konst(v);
            }
        }
        let typed = |d: &Self, i: usize| d.ty(args[i]) == ty;
        match op {
            Op::Multiply => {
                for (a, b) in [(0, 1), (1, 0)] {
                    if self.is_splat(args[a], 1.0) && typed(self, b) {
                        return args[b];
                    }
                    if self.is_splat(args[a], 0.0) {
                        return self.konst(Value::zero(ty));
                    }
                }
            }
            Op::Add => {
                for (a, b) in [(0, 1), (1, 0)] {
                    if self.is_splat(args[a], 0.0) && typed(self, b) {
                        return args[b];
                    }
                }
            }
            Op::Subtract | Op::Divide => {
                let identity = if op == Op::Subtract { 0.0 } else { 1.0 };
                if self.is_splat(args[1], identity) && typed(self, 0) {
                    return args[0];
                }
            }
            Op::IfGreater => {
                if let (Some(a), Some(b)) = (self.known(args[0]), self.known(args[1])) {
                    let pick = if a.floats()[0] > b.floats()[0] { args[2] } else { args[3] };
                    if self.ty(pick) == ty {
                        return pick;
                    }
                }
            }
            _ => {}
        }
        self.dag.add(Node { kind: NodeKind::Op { op, args: args.to_vec() }, ty, origin: ORIGIN.to_string() })
    }

    /// The type of a binary op over `a` and `b`: the one with more components (a scalar broadcasts).
    fn wider(&self, a: NodeId, b: NodeId) -> Type {
        let (ta, tb) = (self.ty(a), self.ty(b));
        if tb.component_count() > ta.component_count() { tb } else { ta }
    }

    fn binary(&mut self, op: Op, a: NodeId, b: NodeId) -> NodeId {
        let ty = self.wider(a, b);
        self.op(op, &[a, b], ty)
    }

    fn mul(&mut self, a: NodeId, b: NodeId) -> NodeId {
        self.binary(Op::Multiply, a, b)
    }

    fn add(&mut self, a: NodeId, b: NodeId) -> NodeId {
        self.binary(Op::Add, a, b)
    }

    fn sub(&mut self, a: NodeId, b: NodeId) -> NodeId {
        self.binary(Op::Subtract, a, b)
    }

    fn div(&mut self, a: NodeId, b: NodeId) -> NodeId {
        self.binary(Op::Divide, a, b)
    }

    fn max(&mut self, a: NodeId, b: NodeId) -> NodeId {
        self.binary(Op::Max, a, b)
    }

    fn min(&mut self, a: NodeId, b: NodeId) -> NodeId {
        self.binary(Op::Min, a, b)
    }

    fn sqrt(&mut self, a: NodeId) -> NodeId {
        let ty = self.ty(a);
        self.op(Op::Sqrt, &[a], ty)
    }

    fn clamp(&mut self, a: NodeId, low: f32, high: f32) -> NodeId {
        let ty = self.ty(a);
        let l = self.konst(Value::splat(ty, low));
        let h = self.konst(Value::splat(ty, high));
        self.op(Op::Clamp, &[a, l, h], ty)
    }

    fn one_minus(&mut self, a: NodeId) -> NodeId {
        let one = self.float(1.0);
        self.sub(one, a)
    }

    /// `then` where `a > 0`, else `otherwise`.
    fn if_positive(&mut self, a: NodeId, then: NodeId, otherwise: NodeId) -> NodeId {
        let zero = self.float(0.0);
        let ty = self.ty(otherwise);
        self.op(Op::IfGreater, &[a, zero, then, otherwise], ty)
    }

    /// `num / den`, or `default` where `den` is not positive.
    fn ratio(&mut self, num: NodeId, den: NodeId, default: NodeId) -> NodeId {
        let eps = self.float(EPSILON);
        let safe = self.max(den, eps);
        let q = self.div(num, safe);
        self.if_positive(den, q, default)
    }

    fn sum(&mut self, ids: &[NodeId]) -> Option<NodeId> {
        let mut out: Option<NodeId> = None;
        for id in ids {
            out = Some(match out {
                Some(acc) => self.add(acc, *id),
                None => *id,
            });
        }
        out
    }

    fn sum_or_zero(&mut self, ids: &[NodeId]) -> NodeId {
        match self.sum(ids) {
            Some(s) => s,
            None => self.float(0.0),
        }
    }

    fn extract(&mut self, a: NodeId, i: u32) -> NodeId {
        self.op(Op::Extract(i), &[a], Type::Float)
    }

    fn average3(&mut self, a: NodeId) -> NodeId {
        if self.ty(a) == Type::Float {
            return a;
        }
        let parts: Vec<NodeId> = (0..3).map(|i| self.extract(a, i)).collect();
        let s = self.sum(&parts).expect("three parts");
        let third = self.float(1.0 / 3.0);
        self.mul(s, third)
    }

    fn max3(&mut self, a: NodeId) -> NodeId {
        let (r, g, b) = (self.extract(a, 0), self.extract(a, 1), self.extract(a, 2));
        let rg = self.max(r, g);
        self.max(rg, b)
    }

    /// The ior whose normal-incidence reflectance is `f0`.
    fn ior_from_f0(&mut self, f0: NodeId) -> NodeId {
        let f0 = self.clamp(f0, 0.0, 0.99999);
        let s = self.sqrt(f0);
        let one = self.float(1.0);
        let up = self.add(one, s);
        let down = self.sub(one, s);
        self.div(up, down)
    }

    /// The normal-incidence reflectance of a dielectric of ior `n`.
    fn f0_from_ior(&mut self, n: NodeId) -> NodeId {
        let one = self.float(1.0);
        let a = self.sub(n, one);
        let b = self.add(n, one);
        let q = self.div(a, b);
        self.mul(q, q)
    }

    /// `((n − 1)² + k²) / ((n + 1)² + k²)` per channel: a conductor's normal-incidence reflectivity.
    fn conductor_f0(&mut self, n: NodeId, k: NodeId) -> NodeId {
        let one = self.float(1.0);
        let k2 = self.mul(k, k);
        let a = self.sub(n, one);
        let a2 = self.mul(a, a);
        let b = self.add(n, one);
        let b2 = self.mul(b, b);
        let up = self.add(a2, k2);
        let down = self.add(b2, k2);
        self.div(up, down)
    }

    /// The roughness and anisotropy whose `open_pbr_anisotropy` is the alpha pair `a`.
    fn invert_alpha(&mut self, a: NodeId) -> (NodeId, NodeId) {
        let (x, y) = (self.extract(a, 0), self.extract(a, 1));
        let hi = self.max(x, y);
        let lo = self.min(x, y);
        let one = self.float(1.0);
        let ratio = self.ratio(lo, hi, one);
        let anisotropy = self.one_minus(ratio);
        let r2 = self.mul(ratio, ratio);
        let half = self.float(0.5);
        let sum = self.add(one, r2);
        let scaled = self.mul(sum, half);
        let k = self.sqrt(scaled);
        let alpha = self.mul(hi, k);
        (self.sqrt(alpha), anisotropy)
    }

    /// The weighted average of `avg`, or `default` where its weights sum to zero. A term of zero weight is left
    /// out rather than multiplied by zero: a lobe input computed by a division (glTF's normalised sheen color) may
    /// be NaN where its weight is zero.
    fn resolve(&mut self, avg: &Avg, default: NodeId) -> NodeId {
        let Some(first) = avg.items.first().map(|i| i.value) else { return default };
        let weights: Vec<NodeId> = avg.items.iter().map(|i| i.w).collect();
        let w = self.sum(&weights).expect("an item");
        if avg.items.iter().all(|i| i.num.is_none() && self.same(i.value, first)) {
            return self.if_positive(w, first, default);
        }
        let mut nums = Vec::new();
        for i in &avg.items {
            let term = match i.num {
                Some(n) => n,
                None => self.mul(i.w, i.value),
            };
            let leaf = !matches!(self.dag.node(i.value).kind, NodeKind::Op { .. });
            nums.push(if leaf {
                term
            } else {
                let zero = self.konst(Value::zero(self.ty(term)));
                self.if_positive(i.w, term, zero)
            });
        }
        let n = self.sum(&nums).expect("an item");
        let ty = self.ty(default);
        let n = if self.ty(n) == ty { n } else { self.op(Op::Convert, &[n], ty) };
        self.ratio(n, w, default)
    }

    /// Whether two nodes hold the same value: one node, equal constants, the same read or uniform.
    fn same(&self, a: NodeId, b: NodeId) -> bool {
        if a == b {
            return true;
        }
        match (&self.dag.node(a).kind, &self.dag.node(b).kind) {
            (NodeKind::Const(x), NodeKind::Const(y)) => x == y,
            (NodeKind::Context(x), NodeKind::Context(y)) => x == y,
            (NodeKind::Uniform { name: x, .. }, NodeKind::Uniform { name: y, .. }) => x == y,
            _ => false,
        }
    }

    fn note(&mut self, text: String) {
        let text = format!("{NOTE_PREFIX}{text}");
        if !self.notes.contains(&text) {
            self.notes.push(text);
        }
    }

    // ---- the tree ----

    fn field(&self, name: &str) -> Option<NodeId> {
        self.field_nodes.get(name).copied()
    }

    /// Input `name` of lobe `id`; `None` when it has none.
    fn input(&self, id: ClosureId, name: &str) -> Option<NodeId> {
        match &self.graph.nodes[id as usize] {
            ClosureNode::Lobe { inputs, .. } => inputs.get(name).and_then(|f| self.field(f)),
            _ => None,
        }
    }

    fn input_or(&mut self, id: ClosureId, name: &str, default: Value) -> NodeId {
        match self.input(id, name) {
            Some(n) => n,
            None => self.konst(default),
        }
    }

    fn weight(&mut self, id: ClosureId) -> NodeId {
        self.input_or(id, "weight", Value::Float(1.0))
    }

    /// Whether Schlick lobe `id` is a Fresnel ramp: its `color0` and `color90` differ.
    fn ramp(&self, id: ClosureId) -> bool {
        match (self.input(id, "color0"), self.input(id, "color90")) {
            (Some(a), Some(b)) => !self.same(a, b),
            (None, None) => false,
            _ => true,
        }
    }

    /// Whether lobe `id` reflects as a dielectric layer: a reflecting dielectric, or (in a layer's top) a reflecting
    /// Schlick ramp.
    fn dielectric_lobe(&self, id: ClosureId, in_top: bool) -> bool {
        let ClosureNode::Lobe { kind, settings, .. } = &self.graph.nodes[id as usize] else { return false };
        match class(kind) {
            Class::Dielectric => scatter(settings).0,
            Class::Schlick => in_top && scatter(settings).0 && self.ramp(id),
            _ => false,
        }
    }

    /// Whether the tree at `id` has a lobe reflecting as a dielectric layer.
    fn reflects(&mut self, id: ClosureId, in_top: bool) -> bool {
        if let Some(found) = self.reflects.get(&(id, in_top)) {
            return *found;
        }
        let found = match self.graph.nodes[id as usize].clone() {
            ClosureNode::Lobe { .. } => self.dielectric_lobe(id, in_top),
            ClosureNode::Layer { top, base } => self.reflects(top, true) || self.reflects(base, in_top),
            ClosureNode::Mix { fg: a, bg: b, .. } | ClosureNode::Add { in1: a, in2: b } => {
                self.reflects(a, in_top) || self.reflects(b, in_top)
            }
            ClosureNode::Multiply { in1, .. } => self.reflects(in1, in_top),
            ClosureNode::Empty { .. } => false,
        };
        self.reflects.insert((id, in_top), found);
        found
    }

    /// Whether every lobe of the tree at `id` is a metal.
    fn metal_only(&self, id: ClosureId, in_top: bool) -> bool {
        match &self.graph.nodes[id as usize] {
            ClosureNode::Lobe { kind, .. } => match class(kind) {
                Class::Conductor => true,
                Class::Schlick => !self.dielectric_lobe(id, in_top),
                _ => false,
            },
            ClosureNode::Layer { top, base } => self.metal_only(*top, true) && self.metal_only(*base, in_top),
            ClosureNode::Mix { fg: a, bg: b, .. } | ClosureNode::Add { in1: a, in2: b } => {
                self.metal_only(*a, in_top) && self.metal_only(*b, in_top)
            }
            ClosureNode::Multiply { in1, .. } => self.metal_only(*in1, in_top),
            ClosureNode::Empty { .. } => true,
        }
    }

    /// Whether the tree at `id` has a transmitting lobe.
    fn transmits(&self, id: ClosureId) -> bool {
        match &self.graph.nodes[id as usize] {
            ClosureNode::Lobe { kind, settings, .. } => {
                matches!(class(kind), Class::Dielectric | Class::Schlick) && scatter(settings).1
            }
            node => node.children().into_iter().any(|c| self.transmits(c)),
        }
    }

    /// The role of the top of a layer reached in `outer`: the coat when it reflects as a dielectric, transmits
    /// nothing, and lies over a base that already reflects as one (or is bare metal); the specular layer when it
    /// reflects as a dielectric otherwise.
    fn top_role(&mut self, top: ClosureId, base: ClosureId, outer: Role) -> Role {
        if !self.reflects(top, true) {
            return outer;
        }
        let in_top = outer != Role::Base;
        let coat = !self.transmits(top) && (self.reflects(base, in_top) || self.metal_only(base, in_top));
        if coat { Role::Coat } else { Role::Spec }
    }

    /// What of a base under it the tree at `id` (in `role`) hides: the weights of its lobes open_pbr_surface does not
    /// layer (diffuse, subsurface, metal, transmission), combined as the flow is. `None` hides nothing.
    fn cover(&mut self, id: ClosureId, role: Role) -> Option<NodeId> {
        match self.graph.nodes[id as usize].clone() {
            ClosureNode::Lobe { kind, settings, .. } => match class(&kind) {
                Class::Diffuse | Class::Subsurface | Class::Translucent | Class::Conductor => Some(self.weight(id)),
                Class::Dielectric | Class::Schlick if scatter(&settings).1 => Some(self.weight(id)),
                Class::Schlick if !self.dielectric_lobe(id, role != Role::Base) => Some(self.weight(id)),
                _ => None,
            },
            ClosureNode::Mix { fg, bg, mix } => {
                let a = self.cover(fg, role);
                let b = self.cover(bg, role);
                if a.is_none() && b.is_none() {
                    return None;
                }
                let m = self.mix_weight(&mix);
                let rest = self.one_minus(m);
                let zero = self.float(0.0);
                let a = self.mul(a.unwrap_or(zero), m);
                let b = self.mul(b.unwrap_or(zero), rest);
                Some(self.add(a, b))
            }
            ClosureNode::Add { in1, in2 } => match (self.cover(in1, role), self.cover(in2, role)) {
                (Some(a), Some(b)) => Some(self.add(a, b)),
                (one, None) | (None, one) => one,
            },
            ClosureNode::Multiply { in1, in2 } => {
                let c = self.cover(in1, role)?;
                match self.field(&in2) {
                    Some(k) if self.ty(k) == Type::Float => Some(self.mul(c, k)),
                    _ => Some(c),
                }
            }
            ClosureNode::Layer { top, base } => {
                let r = self.top_role(top, base, role);
                match (self.cover(top, r), self.cover(base, role)) {
                    (Some(a), Some(b)) => {
                        let a = self.clamp(a, 0.0, 1.0);
                        let through = self.one_minus(a);
                        let hidden = self.mul(through, b);
                        Some(self.add(a, hidden))
                    }
                    (one, None) | (None, one) => one,
                }
            }
            ClosureNode::Empty { .. } => None,
        }
    }

    fn mix_weight(&mut self, field: &str) -> NodeId {
        let m = match self.field(field) {
            Some(m) => m,
            None => self.float(0.5),
        };
        self.clamp(m, 0.0, 1.0)
    }

    fn scale(&mut self, flow: Flow, k: NodeId) -> Flow {
        let s = self.mul(flow.s, k);
        let t = flow.t.map(|t| self.mul(t, k));
        Flow { s, t }
    }

    fn tint(&mut self, flow: Flow, color: NodeId) -> Flow {
        let base = flow.t.unwrap_or(flow.s);
        Flow { s: flow.s, t: Some(self.mul(base, color)) }
    }

    fn merge(&mut self, a: Flow, b: Flow) -> Flow {
        let s = self.add(a.s, b.s);
        let t = match (a.t, b.t) {
            (None, None) => None,
            (x, y) => {
                let x = x.unwrap_or(a.s);
                let y = y.unwrap_or(b.s);
                Some(self.add(x, y))
            }
        };
        Flow { s, t }
    }

    fn send(&mut self, incoming: &mut BTreeMap<(ClosureId, Role), Flow>, to: (ClosureId, Role), flow: Flow) {
        let merged = match incoming.remove(&to) {
            Some(old) => self.merge(old, flow),
            None => flow,
        };
        incoming.insert(to, merged);
    }

    /// Every lobe reached, with its role and flow, in node order.
    fn walk(&mut self) -> Vec<(ClosureId, Role, Flow)> {
        let mut incoming: BTreeMap<(ClosureId, Role), Flow> = BTreeMap::new();
        for root in [self.graph.bsdf, self.graph.edf].into_iter().flatten() {
            let one = self.float(1.0);
            self.send(&mut incoming, (root, Role::Base), Flow { s: one, t: None });
        }
        let mut lobes = Vec::new();
        for id in (0..self.graph.nodes.len() as ClosureId).rev() {
            for role in ROLES {
                let Some(flow) = incoming.remove(&(id, role)) else { continue };
                match self.graph.nodes[id as usize].clone() {
                    ClosureNode::Lobe { kind, closures, .. } => {
                        if class(&kind) == Class::SchlickEdf
                            && let Some(base) = closures.get("base")
                        {
                            let color0 = self.input_or(id, "color0", Value::Color3([1.0; 3]));
                            let tinted = self.tint(flow, color0);
                            self.send(&mut incoming, (*base, role), tinted);
                        }
                        lobes.push((id, role, flow));
                    }
                    ClosureNode::Mix { fg, bg, mix } => {
                        let m = self.mix_weight(&mix);
                        let rest = self.one_minus(m);
                        let a = self.scale(flow, m);
                        let b = self.scale(flow, rest);
                        self.send(&mut incoming, (fg, role), a);
                        self.send(&mut incoming, (bg, role), b);
                    }
                    ClosureNode::Add { in1, in2 } => {
                        self.send(&mut incoming, (in1, role), flow);
                        self.send(&mut incoming, (in2, role), flow);
                    }
                    ClosureNode::Multiply { in1, in2 } => {
                        let next = match self.field(&in2) {
                            Some(k) if self.ty(k) == Type::Float => self.scale(flow, k),
                            Some(k) if self.ty(k) == Type::Color3 => self.tint(flow, k),
                            Some(k) => {
                                let gray = self.average3(k);
                                self.scale(flow, gray)
                            }
                            None => flow,
                        };
                        self.send(&mut incoming, (in1, role), next);
                    }
                    ClosureNode::Layer { top, base } => {
                        let r = self.top_role(top, base, role);
                        let under = match self.cover(top, r) {
                            Some(c) => {
                                let c = self.clamp(c, 0.0, 1.0);
                                let through = self.one_minus(c);
                                self.scale(flow, through)
                            }
                            None => flow,
                        };
                        self.send(&mut incoming, (top, r), flow);
                        self.send(&mut incoming, (base, role), under);
                    }
                    ClosureNode::Empty { .. } => {}
                }
            }
        }
        lobes.reverse();
        lobes
    }

    /// Adds to `avg` a color `value` at coverage `s·k` (`k` the lobe's weight; `None` for one), tinted by the flow.
    fn colored(&mut self, avg: &mut Avg, flow: Flow, k: Option<NodeId>, value: NodeId) {
        let w = match k {
            Some(k) => self.mul(flow.s, k),
            None => flow.s,
        };
        let num = match flow.t {
            Some(t) => {
                let t = match k {
                    Some(k) => self.mul(t, k),
                    None => t,
                };
                Some(self.mul(t, value))
            }
            None => None,
        };
        avg.items.push(Item { w, num, value });
    }

    fn geometry(&mut self, id: ClosureId, tangent: bool, coat: bool, f: &mut Families) {
        if let Some(n) = self.input(id, "normal") {
            if coat { f.coat_normals.push(n) } else { f.normals.push(n) }
        }
        if tangent && let Some(t) = self.input(id, "tangent") {
            if coat { f.coat_tangents.push(t) } else { f.tangents.push(t) }
        }
    }

    fn thin_film(&mut self, id: ClosureId, w: NodeId, f: &mut Families) {
        if let Some(thickness) = self.input(id, "thinfilm_thickness") {
            let ior = self.input_or(id, "thinfilm_ior", Value::Float(1.5));
            f.thin_film.push((w, thickness, ior));
        }
    }

    /// A dielectric reflection (`color` its tint, `ior` its ior) in the specular layer, or in the coat.
    #[allow(clippy::too_many_arguments)]
    fn reflection(
        &mut self,
        id: ClosureId,
        coat: bool,
        flow: Flow,
        wt: NodeId,
        color: NodeId,
        ior: NodeId,
        f: &mut Families,
    ) {
        let e = self.mul(flow.s, wt);
        let roughness = self.input_or(id, "roughness", Value::Vector2([0.05, 0.05]));
        if coat {
            f.coat.push(e);
            self.colored(&mut f.coat_color, flow, Some(wt), color);
            f.coat_ior.push(e, ior);
            f.coat_alpha.push(e, roughness);
        } else {
            f.specular.push(e);
            self.colored(&mut f.specular_color, flow, Some(wt), color);
            f.ior.push(e, ior);
            f.alpha.push(e, roughness);
            self.thin_film(id, e, f);
        }
        self.geometry(id, true, coat, f);
    }

    fn transmission(&mut self, id: ClosureId, flow: Flow, wt: NodeId, color: NodeId, ior: NodeId, f: &mut Families) {
        let e = self.mul(flow.s, wt);
        f.transmission.push(e);
        self.colored(&mut f.transmission_color, flow, Some(wt), color);
        f.ior.push(e, ior);
        let roughness = self.input_or(id, "roughness", Value::Vector2([0.05, 0.05]));
        f.alpha.push(e, roughness);
        self.geometry(id, true, false, f);
    }

    /// A metal of normal-incidence reflectivity `f0` (and edge tint `edge`, a Schlick lobe's `color82`).
    fn metal(&mut self, id: ClosureId, flow: Flow, f0: NodeId, edge: Option<NodeId>, f: &mut Families) {
        let wt = self.weight(id);
        f.metal.push(flow.s);
        let reflectivity = self.mul(wt, f0);
        self.colored(&mut f.base_color, flow, None, reflectivity);
        // An edge tint that is the face color (MDL's tinted glossy lobe) is no edge tint: leaving it out keeps the
        // dielectric's specular color its own. Another is weighted by the metal's flow alone.
        if let Some(edge) = edge.filter(|e| !self.same(*e, f0)) {
            self.colored(&mut f.specular_color, flow, None, edge);
        }
        let roughness = self.input_or(id, "roughness", Value::Vector2([0.05, 0.05]));
        f.alpha.push(flow.s, roughness);
        self.thin_film(id, flow.s, f);
        self.geometry(id, true, false, f);
    }

    fn lobe(&mut self, id: ClosureId, role: Role, flow: Flow, f: &mut Families) {
        let ClosureNode::Lobe { kind, ty, settings, inputs, .. } = self.graph.nodes[id as usize].clone() else {
            return;
        };
        let white = Value::Color3([1.0; 3]);
        if let Some(name) = inputs.keys().find(|n| n.contains("dispersion")) {
            self.note(format!("{kind}: {name} has no open_pbr_surface field on the lobe, dispersion dropped"));
        }
        match class(&kind) {
            Class::Diffuse => {
                let wt = self.weight(id);
                let e = self.mul(flow.s, wt);
                f.diffuse.push(e);
                let color = self.input_or(id, "color", Value::Color3([0.18; 3]));
                self.colored(&mut f.base_color, flow, Some(wt), color);
                let roughness = self.input_or(id, "roughness", Value::Float(0.0));
                f.diffuse_roughness.push(e, roughness);
                self.geometry(id, false, false, f);
            }
            Class::Subsurface | Class::Translucent => {
                let wt = self.weight(id);
                let e = self.mul(flow.s, wt);
                f.subsurface.push(e);
                let subsurface = class(&kind) == Class::Subsurface;
                let color = self.input_or(id, "color", if subsurface { Value::Color3([0.18; 3]) } else { white });
                self.colored(&mut f.subsurface_color, flow, Some(wt), color);
                if subsurface {
                    let radius = self.input_or(id, "radius", Value::Color3([1.0; 3]));
                    f.subsurface_radius.push(e, radius);
                    let anisotropy = self.input_or(id, "anisotropy", Value::Float(0.0));
                    f.subsurface_anisotropy.push(e, anisotropy);
                }
                self.geometry(id, false, false, f);
            }
            Class::Dielectric => {
                let (reflect, transmit) = scatter(&settings);
                let wt = self.weight(id);
                let tint = self.input_or(id, "tint", white);
                let ior = self.input_or(id, "ior", Value::Float(1.5));
                if reflect {
                    self.reflection(id, role == Role::Coat, flow, wt, tint, ior, f);
                }
                if transmit {
                    self.transmission(id, flow, wt, tint, ior, f);
                }
            }
            Class::Conductor => {
                let n = self.input_or(id, "ior", Value::Color3([0.183, 0.421, 1.373]));
                let k = self.input_or(id, "extinction", Value::Color3([3.424, 2.346, 1.770]));
                let f0 = self.conductor_f0(n, k);
                self.metal(id, flow, f0, None, f);
            }
            Class::Schlick => {
                let (reflect, transmit) = scatter(&settings);
                let color0 = self.input_or(id, "color0", white.clone());
                let color90 = self.input_or(id, "color90", white);
                let dielectric = self.dielectric_lobe(id, role != Role::Base);
                let wt = self.weight(id);
                let ior = if dielectric || transmit {
                    let a = self.average3(color0);
                    let b = self.average3(color90);
                    let one = self.float(1.0);
                    let f0 = self.ratio(a, b, one);
                    Some(self.ior_from_f0(f0))
                } else {
                    None
                };
                if let Some(ior) = ior.filter(|_| dielectric) {
                    self.reflection(id, role == Role::Coat, flow, wt, color90, ior, f);
                } else if reflect {
                    let edge = self.input(id, "color82");
                    self.metal(id, flow, color0, edge, f);
                }
                if let Some(ior) = ior.filter(|_| transmit) {
                    self.transmission(id, flow, wt, color90, ior, f);
                }
            }
            Class::Sheen => {
                let wt = self.weight(id);
                let e = self.mul(flow.s, wt);
                f.fuzz.push(e);
                let color = self.input_or(id, "color", white);
                self.colored(&mut f.fuzz_color, flow, Some(wt), color);
                let roughness = self.input_or(id, "roughness", Value::Float(0.3));
                f.fuzz_roughness.push(e, roughness);
            }
            Class::Emission => {
                if kind != "uniform_edf" {
                    self.note(format!("{kind} as uniform emission"));
                }
                let color = self.input_or(id, "color", white);
                let k = flow.t.unwrap_or(flow.s);
                let radiance = self.mul(k, color);
                f.emission.push(radiance);
            }
            // The walk sent its base on, tinted.
            Class::SchlickEdf => {}
            Class::Other => match ty {
                ClosureType::Vdf => self.note(format!("{kind}: a medium has no open_pbr_surface field, dropped")),
                _ => self.note(format!("{kind}: no open_pbr_surface field, dropped")),
            },
        }
    }

    /// The first of `values`, with a warning naming `what` when the others differ.
    fn first(&mut self, values: &[NodeId], what: &str) -> Option<NodeId> {
        let first = *values.first()?;
        if values.iter().any(|v| !self.same(*v, first)) {
            self.note(format!("lobes with different {what}s, the first lobe's"));
        }
        Some(first)
    }

    fn distill_fields(&mut self) -> HashMap<String, NodeId> {
        let mut f = Families::default();
        for (id, role, flow) in self.walk() {
            self.lobe(id, role, flow, &mut f);
        }
        let mut out: HashMap<String, NodeId> = HashMap::new();
        let zero = self.float(0.0);
        let one = self.float(1.0);
        let white = self.konst(Value::Color3([1.0; 3]));

        // The base: metal, transmission, subsurface and diffuse shares, normalised.
        let d = self.sum_or_zero(&f.diffuse);
        let ss = self.sum_or_zero(&f.subsurface);
        let t = self.sum_or_zero(&f.transmission);
        let m = self.sum_or_zero(&f.metal);
        let c = self.sum(&[d, ss, t, m]).expect("four terms");
        let n = self.max(c, one);
        let k = self.sub(n, c);
        out.insert("base_metalness".into(), self.div(m, n));
        let dielectric = self.sub(n, m);
        if !f.transmission.is_empty() {
            out.insert("transmission_weight".into(), self.ratio(t, dielectric, zero));
        }
        if !f.subsurface.is_empty() {
            let opaque = self.sub(dielectric, t);
            out.insert("subsurface_weight".into(), self.ratio(ss, opaque, zero));
        }
        let dk = self.add(d, k);
        out.insert("base_weight".into(), self.ratio(d, dk, one));
        let default = self.konst(Value::Color3([0.8; 3]));
        out.insert("base_color".into(), self.resolve(&f.base_color, default));
        out.insert("base_diffuse_roughness".into(), self.resolve(&f.diffuse_roughness, zero));

        // The specular layer and the dielectric interface.
        let default_ior = self.float(1.5);
        let ior = self.resolve(&f.ior, default_ior);
        let has_transmission = !f.transmission.is_empty();
        let coverage = if f.specular.is_empty() {
            None
        } else {
            let cov = self.sum_or_zero(&f.specular);
            let share = self.ratio(cov, dielectric, one);
            Some(self.clamp(share, 0.0, 1.0))
        };
        let (weight, ior) = match (!f.metal.is_empty(), coverage) {
            (true, Some(w)) if self.is_splat(w, 1.0) => (one, ior),
            (true, Some(w)) => {
                let f0 = self.f0_from_ior(ior);
                let scaled = self.mul(w, f0);
                (one, self.ior_from_f0(scaled))
            }
            (true, None) if has_transmission => (one, ior),
            (true, None) => (one, one),
            (false, Some(w)) => (w, ior),
            (false, None) if has_transmission => (one, ior),
            (false, None) => (zero, ior),
        };
        out.insert("specular_weight".into(), weight);
        out.insert("specular_ior".into(), ior);
        out.insert("specular_color".into(), self.resolve(&f.specular_color, white));
        if !f.alpha.items.is_empty() {
            let default = self.konst(Value::Vector2([0.09, 0.09]));
            let alpha = self.resolve(&f.alpha, default);
            let (r, a) = self.invert_alpha(alpha);
            out.insert("specular_roughness".into(), r);
            out.insert("specular_roughness_anisotropy".into(), a);
        }
        if has_transmission {
            out.insert("transmission_color".into(), self.resolve(&f.transmission_color, white));
        }

        // Subsurface.
        if !f.subsurface.is_empty() {
            let default = self.konst(Value::Color3([0.8; 3]));
            out.insert("subsurface_color".into(), self.resolve(&f.subsurface_color, default));
            if !f.subsurface_radius.items.is_empty() {
                let default_scale = self.konst(Value::Color3([1.0, 0.5, 0.25]));
                let radius = self.resolve(&f.subsurface_radius, default_scale);
                let r = self.max3(radius);
                out.insert("subsurface_radius".into(), r);
                out.insert("subsurface_radius_scale".into(), self.ratio(radius, r, default_scale));
                out.insert("subsurface_scatter_anisotropy".into(), self.resolve(&f.subsurface_anisotropy, zero));
            }
        }

        // Coat.
        if !f.coat.is_empty() {
            let w = self.sum_or_zero(&f.coat);
            out.insert("coat_weight".into(), self.clamp(w, 0.0, 1.0));
            out.insert("coat_color".into(), self.resolve(&f.coat_color, white));
            let default = self.float(1.6);
            out.insert("coat_ior".into(), self.resolve(&f.coat_ior, default));
            let default = self.konst(Value::Vector2([0.0, 0.0]));
            let alpha = self.resolve(&f.coat_alpha, default);
            let (r, a) = self.invert_alpha(alpha);
            out.insert("coat_roughness".into(), r);
            out.insert("coat_roughness_anisotropy".into(), a);
        }

        // Fuzz.
        if !f.fuzz.is_empty() {
            let w = self.sum_or_zero(&f.fuzz);
            out.insert("fuzz_weight".into(), self.clamp(w, 0.0, 1.0));
            out.insert("fuzz_color".into(), self.resolve(&f.fuzz_color, white));
            let default = self.float(0.5);
            out.insert("fuzz_roughness".into(), self.resolve(&f.fuzz_roughness, default));
        }

        // Thin film: the share of the specular and metal lobes that carry one.
        if f.thin_film.iter().any(|(_, thickness, _)| !self.is_splat(*thickness, 0.0)) {
            let mut on = Vec::new();
            let mut thickness = Avg::default();
            let mut film_ior = Avg::default();
            for (w, th, ior) in f.thin_film.clone() {
                let has = self.if_positive(th, one, zero);
                let w_on = self.mul(w, has);
                on.push(w_on);
                thickness.push(w_on, th);
                film_ior.push(w_on, ior);
            }
            let weights: Vec<NodeId> = f.thin_film.iter().map(|(w, _, _)| *w).collect();
            let total = self.sum_or_zero(&weights);
            let covered = self.sum_or_zero(&on);
            out.insert("thin_film_weight".into(), self.ratio(covered, total, zero));
            let default = self.float(500.0);
            let nm = self.resolve(&thickness, default);
            let per_micron = self.float(1000.0);
            out.insert("thin_film_thickness".into(), self.div(nm, per_micron));
            let default = self.float(1.4);
            out.insert("thin_film_ior".into(), self.resolve(&film_ior, default));
        }

        // Emission: the radiance as its brightest channel and a color.
        if let Some(radiance) = self.sum(&f.emission) {
            let radiance = if self.ty(radiance) == Type::Color3 {
                radiance
            } else {
                self.op(Op::Convert, &[radiance], Type::Color3)
            };
            let luminance = self.max3(radiance);
            out.insert("emission_luminance".into(), luminance);
            out.insert("emission_color".into(), self.ratio(radiance, luminance, white));
        }

        // Geometry.
        if let Some(opacity) = self.graph.opacity.as_deref().and_then(|o| self.field(o)) {
            out.insert("geometry_opacity".into(), opacity);
        }
        if let Some(thin) = self.field("thin_walled") {
            out.insert("geometry_thin_walled".into(), thin);
        }
        for (values, what, name) in [
            (std::mem::take(&mut f.normals), "normal", "geometry_normal"),
            (std::mem::take(&mut f.tangents), "tangent", "geometry_tangent"),
            (std::mem::take(&mut f.coat_normals), "coat normal", "geometry_coat_normal"),
            (std::mem::take(&mut f.coat_tangents), "coat tangent", "geometry_coat_tangent"),
        ] {
            if let Some(v) = self.first(&values, what) {
                out.insert(name.into(), v);
            }
        }
        out
    }
}
