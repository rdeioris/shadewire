//! From the model struct to Godot's spatial outputs, per model (the tables of the Godot analysis,
//! Section 3). Godot's lighting has no IOR, specular color, transmission, thin film or sheen, so
//! each model maps what it can and lists every field it drops or approximates.

use shadewire::{ContextRead, Dag, NodeKind, Reflection, Space, SurfaceField, Type, Value};

use crate::names::{SURFACE, member as ident};
use crate::options::{GdshaderOptions, Transparency};
use crate::types::{cast, literal};

/// The Rec. 709 luminance and `SPECULAR` from IOR, weight and tint: Godot's dielectric
/// F0 is `0.16 * SPECULAR^2`. Parameters keep the `shadewire_` prefix, which no uniform takes.
pub const HELPERS: &str = "\
float shadewire_luminance(vec3 shadewire_c) {
    return dot(shadewire_c, vec3(0.2126, 0.7152, 0.0722));
}

float shadewire_specular(float shadewire_ior, float shadewire_weight, vec3 shadewire_tint) {
    float shadewire_r = (shadewire_ior - 1.0) / (shadewire_ior + 1.0);
    float shadewire_f0 = shadewire_r * shadewire_r * shadewire_weight * shadewire_luminance(shadewire_tint);
    return clamp(sqrt(shadewire_f0 / 0.16), 0.0, 1.0);
}
";

/// What `fragment()` writes after evaluating the surface, and what it costs.
pub struct Mapping {
    pub render_modes: Vec<&'static str>,
    pub lines: Vec<String>,
    /// Whether the lines call `HELPERS`.
    pub helpers: bool,
    pub approximations: Vec<String>,
}

/// When a model's opacity is a cutout rather than a blend, for `Transparency::Auto`.
#[derive(Clone, Copy)]
enum Cutout<'f> {
    None,
    /// glTF: `alpha_mode` OPAQUE (0) ignores alpha, MASK (1) cuts at `cutoff`, BLEND (2) blends.
    GltfMode {
        mode: &'f str,
        cutoff: &'f str,
    },
    /// UsdPreviewSurface: a threshold above 0 cuts there.
    Threshold(&'f str),
}

struct Builder<'a> {
    dag: &'a Dag,
    reflection: &'a Reflection,
    options: &'a GdshaderOptions,
    out: Mapping,
}

impl<'a> Builder<'a> {
    fn field(&self, name: &str) -> Option<&'a SurfaceField> {
        self.dag.surface.fields.iter().find(|f| f.name == name)
    }

    /// `s.<field>` as `ty`, or the field's neutral value when the model struct lacks it.
    fn s(&self, name: &str, ty: Type, neutral: f32) -> String {
        match self.field(name) {
            Some(f) => cast(&format!("{SURFACE}.{}", ident(&f.name)), f.ty, ty),
            None => literal(&Value::splat(ty, neutral)),
        }
    }

    /// The constant value of a field, when it has one.
    fn constant(&self, name: &str) -> Option<Vec<f32>> {
        let p = self.reflection.params.get(name)?;
        if p.kind != "constant" {
            return None;
        }
        p.value.as_ref().map(Value::floats)
    }

    /// Whether a field is absent or the constant `x` in every channel.
    fn is(&self, name: &str, x: f32) -> bool {
        self.field(name).is_none() || self.constant(name).is_some_and(|v| v.iter().all(|c| *c == x))
    }

    /// Whether a field is absent or the constant `value`, channel by channel.
    fn is_value(&self, name: &str, value: &[f32]) -> bool {
        self.field(name).is_none() || self.constant(name).is_some_and(|v| v == value)
    }

    /// Whether a normal field is absent, the interpolated world normal, or the same node as
    /// the `main` normal field.
    fn plain_normal(&self, name: &str, main: Option<&str>) -> bool {
        let Some(f) = self.field(name) else { return true };
        let same = main.and_then(|m| self.field(m)).is_some_and(|m| m.node == f.node);
        same || matches!(self.dag.node(f.node).kind, NodeKind::Context(ContextRead::Normal(Space::World)))
    }

    fn set(&mut self, output: &str, expr: String) {
        self.out.lines.push(format!("{output} = {expr};"));
    }

    fn note(&mut self, text: String) {
        if !self.out.approximations.contains(&text) {
            self.out.approximations.push(text);
        }
    }

    /// Notes each field that is not at its neutral value: Godot has nowhere to put it.
    fn drop_unless(&mut self, fields: &[(&str, f32)]) {
        for (name, neutral) in fields {
            if !self.is(name, *neutral) {
                self.note(format!("godot: {name} dropped"));
            }
        }
    }

    fn normal(&mut self, name: &str) {
        if !self.plain_normal(name, None) {
            let n = self.s(name, Type::Vector3, 0.0);
            self.set("NORMAL", format!("normalize((VIEW_MATRIX * vec4({n}, 0.0)).xyz)"));
        }
    }

    fn specular(&mut self, ior: &str, weight: &str, tint: &str) {
        let expr = format!(
            "shadewire_specular({}, {}, {})",
            self.s(ior, Type::Float, 1.5),
            self.s(weight, Type::Float, 1.0),
            self.s(tint, Type::Color3, 1.0)
        );
        self.set("SPECULAR", expr);
        self.out.helpers = true;
        if !self.is(tint, 1.0) {
            self.note(format!("godot: {tint} as its luminance"));
        }
    }

    /// CLEARCOAT and CLEARCOAT_ROUGHNESS, unless the coat is off. Godot's coat lobe has a fixed
    /// IOR, no tint and a capped roughness.
    fn coat(&mut self, weight: &str, roughness: &str, dropped: &[(&str, f32)], coat_normal: &str, normal: &str) {
        if self.is(weight, 0.0) {
            return;
        }
        let w = self.s(weight, Type::Float, 0.0);
        let r = self.s(roughness, Type::Float, 0.0);
        self.set("CLEARCOAT", w);
        self.set("CLEARCOAT_ROUGHNESS", r);
        self.note("godot: coat roughness remapped (Godot's coat stays glossy)".to_string());
        self.drop_unless(dropped);
        if !self.plain_normal(coat_normal, Some(normal)) {
            self.note(format!("godot: {coat_normal} dropped"));
        }
    }

    /// ANISOTROPY, and ANISOTROPY_FLOW from a rotation in radians when there is one.
    fn anisotropy(&mut self, strength: &str, angle: Option<String>) {
        if self.is(strength, 0.0) {
            return;
        }
        let a = self.s(strength, Type::Float, 0.0);
        self.set("ANISOTROPY", a);
        if let Some(angle) = angle {
            self.set("ANISOTROPY_FLOW", format!("vec2(cos({angle}), sin({angle}))"));
        }
        self.note(format!("godot: {strength} approximated (Godot's anisotropy model)"));
    }

    fn subsurface(&mut self, weight: &str) {
        if self.is(weight, 0.0) {
            return;
        }
        let w = self.s(weight, Type::Float, 0.0);
        self.set("SSS_STRENGTH", w);
        self.note(format!("godot: {weight} approximated (subsurface color and radius dropped)"));
    }

    /// RIM as a stand-in for a rough sheen or fuzz layer.
    fn rim(&mut self, field: &str, expr: String) {
        if self.is(field, 0.0) {
            return;
        }
        self.set("RIM", expr);
        self.note(format!("godot: {field} approximated as rim light"));
    }

    fn emission(&mut self, amount: &str, color: &str) {
        if self.is(amount, 0.0) || self.is(color, 0.0) {
            return;
        }
        let mut expr = format!("{} * {}", self.s(amount, Type::Float, 0.0), self.s(color, Type::Color3, 0.0));
        if self.options.emission_scale != 1.0 {
            expr.push_str(&format!(" * {}", literal(&Value::Float(self.options.emission_scale))));
        }
        self.set("EMISSION", expr);
    }

    /// ALPHA and the render mode it needs, from the `transparency` option and the model's cutout rule.
    fn alpha(&mut self, opacity: &str, cutout: Cutout) {
        if self.field(opacity).is_none() {
            return;
        }
        let cutoff = match cutout {
            Cutout::None => None,
            Cutout::GltfMode { cutoff, .. } | Cutout::Threshold(cutoff) => Some(cutoff),
        };
        let mode = match self.options.transparency {
            Transparency::Auto if self.is(opacity, 1.0) => Transparency::Opaque,
            Transparency::Auto => match cutout {
                Cutout::GltfMode { mode, .. } => match self.constant(mode) {
                    Some(m) if m[0] == 0.0 => Transparency::Opaque,
                    Some(m) if m[0] == 1.0 => Transparency::Scissor,
                    _ => Transparency::Blend,
                },
                Cutout::Threshold(t) => match self.constant(t) {
                    Some(v) if v[0] > 0.0 => Transparency::Scissor,
                    _ => Transparency::Blend,
                },
                Cutout::None => Transparency::Blend,
            },
            other => other,
        };
        let ty = self.field(opacity).map(|f| f.ty).unwrap_or(Type::Float);
        let a = if ty.is_scalar() {
            self.s(opacity, Type::Float, 1.0)
        } else {
            self.out.helpers = true;
            format!("shadewire_luminance({})", self.s(opacity, Type::Color3, 1.0))
        };
        match mode {
            Transparency::Auto | Transparency::Opaque => {}
            Transparency::Scissor => {
                self.set("ALPHA", a);
                let threshold = cutoff.map_or("0.5".to_string(), |c| self.s(c, Type::Float, 0.5));
                self.set("ALPHA_SCISSOR_THRESHOLD", threshold);
            }
            Transparency::Hash => {
                self.set("ALPHA", a);
                self.set("ALPHA_HASH_SCALE", "1.0".to_string());
            }
            Transparency::Blend => {
                self.set("ALPHA", a);
                self.out.render_modes.extend(["blend_mix", "depth_draw_opaque"]);
            }
        }
    }

    /// `cull_disabled` unless the thin-walled flag is the constant false.
    fn cull(&mut self, thin_walled: Option<&str>) {
        let culled = thin_walled.is_none_or(|t| self.is(t, 0.0));
        self.out.render_modes.push(if culled { "cull_back" } else { "cull_disabled" });
    }
}

pub fn map(dag: &Dag, reflection: &Reflection, options: &GdshaderOptions) -> Mapping {
    let out = Mapping { render_modes: Vec::new(), lines: Vec::new(), helpers: false, approximations: Vec::new() };
    let mut b = Builder { dag, reflection, options, out };
    match dag.surface.model.as_str() {
        "gltf_pbr" => gltf_pbr(&mut b),
        "standard_surface" => standard_surface(&mut b),
        "open_pbr_surface" => open_pbr_surface(&mut b),
        "surface_unlit" => surface_unlit(&mut b),
        "UsdPreviewSurface" => usd_preview_surface(&mut b),
        "disney_principled" => disney_principled(&mut b),
        other => {
            if b.field("base_color").is_some_and(|f| f.ty.component_count() == 3) {
                let c = b.s("base_color", Type::Color3, 0.0);
                b.set("ALBEDO", c);
            }
            b.note(format!("godot: model {other} has no Godot mapping (only base_color is used)"));
        }
    }
    if !b.out.render_modes.iter().any(|m| m.starts_with("cull_")) {
        b.cull(None);
    }
    b.out
}

fn gltf_pbr(b: &mut Builder) {
    let albedo = b.s("base_color", Type::Color3, 1.0);
    b.set("ALBEDO", albedo);
    let metallic = b.s("metallic", Type::Float, 1.0);
    b.set("METALLIC", metallic);
    let roughness = b.s("roughness", Type::Float, 1.0);
    b.set("ROUGHNESS", roughness);
    b.specular("ior", "specular", "specular_color");
    b.normal("normal");
    if !b.is("occlusion", 1.0) {
        let ao = b.s("occlusion", Type::Float, 1.0);
        b.set("AO", ao);
    }
    b.coat("clearcoat", "clearcoat_roughness", &[], "clearcoat_normal", "normal");
    let rotation = b.s("anisotropy_rotation", Type::Float, 0.0);
    b.anisotropy("anisotropy_strength", Some(rotation));
    let sheen = format!("shadewire_luminance({})", b.s("sheen_color", Type::Color3, 0.0));
    if !b.is("sheen_color", 0.0) {
        b.out.helpers = true;
    }
    b.rim("sheen_color", sheen);
    b.emission("emissive_strength", "emissive");
    b.alpha("alpha", Cutout::GltfMode { mode: "alpha_mode", cutoff: "alpha_cutoff" });
    b.drop_unless(&[("transmission", 0.0), ("thickness", 0.0), ("iridescence", 0.0), ("dispersion", 0.0)]);
    b.cull(None);
}

fn standard_surface(b: &mut Builder) {
    let albedo = format!("{} * {}", b.s("base", Type::Float, 1.0), b.s("base_color", Type::Color3, 0.8));
    b.set("ALBEDO", albedo);
    let metallic = b.s("metalness", Type::Float, 0.0);
    b.set("METALLIC", metallic);
    let roughness = b.s("specular_roughness", Type::Float, 0.2);
    b.set("ROUGHNESS", roughness);
    b.specular("specular_IOR", "specular", "specular_color");
    b.normal("normal");
    let coat_dropped = [("coat_color", 1.0), ("coat_IOR", 1.5), ("coat_anisotropy", 0.0)];
    b.coat("coat", "coat_roughness", &coat_dropped, "coat_normal", "normal");
    // MaterialX turns the tangent by specular_rotation * 360 degrees.
    let rotation = format!("{} * 6.28318530718", b.s("specular_rotation", Type::Float, 0.0));
    b.anisotropy("specular_anisotropy", Some(rotation));
    b.subsurface("subsurface");
    let sheen = b.s("sheen", Type::Float, 0.0);
    b.rim("sheen", sheen);
    b.emission("emission", "emission_color");
    b.alpha("opacity", Cutout::None);
    b.drop_unless(&[
        ("diffuse_roughness", 0.0),
        ("transmission", 0.0),
        ("thin_film_thickness", 0.0),
        ("coat_affect_color", 0.0),
        ("coat_affect_roughness", 0.0),
    ]);
    b.cull(Some("thin_walled"));
}

fn open_pbr_surface(b: &mut Builder) {
    let albedo = format!("{} * {}", b.s("base_weight", Type::Float, 1.0), b.s("base_color", Type::Color3, 0.8));
    b.set("ALBEDO", albedo);
    let metallic = b.s("base_metalness", Type::Float, 0.0);
    b.set("METALLIC", metallic);
    let roughness = b.s("specular_roughness", Type::Float, 0.3);
    b.set("ROUGHNESS", roughness);
    b.specular("specular_ior", "specular_weight", "specular_color");
    b.normal("geometry_normal");
    let coat_dropped = [("coat_color", 1.0), ("coat_ior", 1.6), ("coat_roughness_anisotropy", 0.0)];
    b.coat("coat_weight", "coat_roughness", &coat_dropped, "geometry_coat_normal", "geometry_normal");
    // The direction follows the mesh tangent; a custom geometry_tangent is not carried over.
    b.anisotropy("specular_roughness_anisotropy", None);
    b.subsurface("subsurface_weight");
    let fuzz = b.s("fuzz_weight", Type::Float, 0.0);
    b.rim("fuzz_weight", fuzz);
    b.emission("emission_luminance", "emission_color");
    b.alpha("geometry_opacity", Cutout::None);
    b.drop_unless(&[("base_diffuse_roughness", 0.0), ("transmission_weight", 0.0), ("thin_film_weight", 0.0)]);
    b.cull(Some("geometry_thin_walled"));
}

fn surface_unlit(b: &mut Builder) {
    b.out.render_modes.push("unshaded");
    let albedo = format!("{} * {}", b.s("emission", Type::Float, 1.0), b.s("emission_color", Type::Color3, 1.0));
    b.set("ALBEDO", albedo);
    b.alpha("opacity", Cutout::None);
    b.drop_unless(&[("transmission", 0.0)]);
    b.cull(None);
}

/// UsdPreviewSurface 2.6. The metallic workflow takes F0 from `ior`; the specular workflow
/// (`useSpecularWorkflow` 1) takes it from `specularColor` and is never metallic. `normal` is a
/// tangent-space normal in [-1, 1], which Godot reads through NORMAL_MAP.
fn usd_preview_surface(b: &mut Builder) {
    let albedo = b.s("diffuseColor", Type::Color3, 0.18);
    b.set("ALBEDO", albedo);
    let metallic_f0 = format!("shadewire_specular({}, 1.0, vec3(1.0))", b.s("ior", Type::Float, 1.5));
    let specular_f0 =
        format!("clamp(sqrt(shadewire_luminance({}) / 0.16), 0.0, 1.0)", b.s("specularColor", Type::Color3, 0.0));
    let metallic = b.s("metallic", Type::Float, 0.0);
    b.out.helpers = true;
    // Some(true) for the specular workflow, None when it is chosen at run time.
    let workflow = match b.field("useSpecularWorkflow") {
        Some(_) => b.constant("useSpecularWorkflow").map(|w| w[0] != 0.0),
        None => Some(false),
    };
    let (metallic, specular) = match workflow {
        Some(false) => (metallic, metallic_f0),
        Some(true) => ("0.0".to_string(), specular_f0),
        None => {
            let w = format!("{} != 0", b.s("useSpecularWorkflow", Type::Integer, 0.0));
            (format!("({w} ? 0.0 : {metallic})"), format!("({w} ? {specular_f0} : {metallic_f0})"))
        }
    };
    let specular_workflow = workflow != Some(false);
    b.set("METALLIC", metallic);
    let roughness = b.s("roughness", Type::Float, 0.5);
    b.set("ROUGHNESS", roughness);
    b.set("SPECULAR", specular);
    let grey = b.constant("specularColor").is_some_and(|c| c.iter().all(|x| *x == c[0]));
    if specular_workflow && !grey {
        b.note("godot: specularColor as its luminance".to_string());
    }
    if !b.is_value("normal", &[0.0, 0.0, 1.0]) {
        let n = b.s("normal", Type::Vector3, 0.0);
        b.set("NORMAL_MAP", format!("{n} * 0.5 + 0.5"));
    }
    if !b.is("occlusion", 1.0) {
        let ao = b.s("occlusion", Type::Float, 1.0);
        b.set("AO", ao);
    }
    b.coat("clearcoat", "clearcoatRoughness", &[], "", "");
    if !b.is("emissiveColor", 0.0) {
        let mut expr = b.s("emissiveColor", Type::Color3, 0.0);
        if b.options.emission_scale != 1.0 {
            expr.push_str(&format!(" * {}", literal(&Value::Float(b.options.emission_scale))));
        }
        b.set("EMISSION", expr);
    }
    // In the transparent mode (0) MaterialX shades a partly opaque surface as a clear dielectric
    // (transmission); Godot blends instead.
    if !b.is("opacity", 1.0) && b.is("opacityMode", 0.0) && b.is("opacityThreshold", 0.0) {
        b.note("godot: opacity as alpha blending (transmission in the transparent opacityMode)".to_string());
    }
    b.alpha("opacity", Cutout::Threshold("opacityThreshold"));
    b.drop_unless(&[("displacement", 0.0)]);
    b.cull(None);
}

/// The Disney principled BRDF (2012), which Godot's own model follows: its dielectric F0 is
/// `0.08 * specular` (Godot: `0.16 * SPECULAR^2`), and its coat roughness `mix(0.1, 0.001, gloss)`
/// is Godot's `mix(0.001, 0.1, CLEARCOAT_ROUGHNESS)` at `1 - gloss`. `sheenTint` is RIM_TINT.
fn disney_principled(b: &mut Builder) {
    let albedo = b.s("baseColor", Type::Color3, 0.16);
    b.set("ALBEDO", albedo);
    let metallic = b.s("metallic", Type::Float, 0.0);
    b.set("METALLIC", metallic);
    let roughness = b.s("roughness", Type::Float, 0.5);
    b.set("ROUGHNESS", roughness);
    let specular = format!("clamp(sqrt({} * 0.5), 0.0, 1.0)", b.s("specular", Type::Float, 0.5));
    b.set("SPECULAR", specular);
    if !b.is("clearcoat", 0.0) {
        let coat = b.s("clearcoat", Type::Float, 0.0);
        b.set("CLEARCOAT", coat);
        let gloss = b.s("clearcoatGloss", Type::Float, 1.0);
        b.set("CLEARCOAT_ROUGHNESS", format!("1.0 - {gloss}"));
    }
    // The direction follows the mesh tangent; Disney has no rotation input.
    b.anisotropy("anisotropic", None);
    b.subsurface("subsurface");
    if !b.is("sheen", 0.0) {
        let sheen = b.s("sheen", Type::Float, 0.0);
        let tint = b.s("sheenTint", Type::Float, 0.5);
        b.set("RIM", sheen);
        b.set("RIM_TINT", tint);
        b.note("godot: sheen approximated as rim light".to_string());
    }
    b.drop_unless(&[("specularTint", 0.0), ("specTrans", 0.0)]);
    b.cull(None);
}

/// What a closure tree's surface sets besides its lobes: transparency from its `opacity` field and culling from
/// its `thin_walled` field. The lobes themselves run in `light()` ([`crate::lobes`]).
pub fn closure_mapping(dag: &Dag, reflection: &Reflection, options: &GdshaderOptions) -> Mapping {
    let out = Mapping { render_modes: Vec::new(), lines: Vec::new(), helpers: false, approximations: Vec::new() };
    let mut b = Builder { dag, reflection, options, out };
    if let Some(opacity) = dag.surface.closure.as_ref().and_then(|g| g.opacity.clone()) {
        b.alpha(&opacity, Cutout::None);
    }
    b.cull(Some("thin_walled"));
    b.out
}
