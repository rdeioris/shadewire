//! From the model struct to Godot's spatial outputs, per model (the tables of the Godot analysis,
//! Section 3). Godot's lighting has no IOR, specular color, transmission, thin film or sheen, so
//! each model maps what it can and lists every field it drops or approximates.

use shadewire::{ContextRead, Dag, NodeKind, Reflection, Space, SurfaceField, Type, Value};

use super::names::{SURFACE, ident};
use super::ops::cast;
use super::types::literal;
use crate::options::{GdshaderOptions, Transparency};

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

    /// ALPHA and the render mode it needs, from the `transparency` option. `alpha_mode` and
    /// `cutoff` are the glTF fields, absent elsewhere.
    fn alpha(&mut self, opacity: &str, alpha_mode: Option<&str>, cutoff: Option<&str>) {
        if self.field(opacity).is_none() {
            return;
        }
        let mode = match self.options.transparency {
            Transparency::Auto if self.is(opacity, 1.0) => Transparency::Opaque,
            Transparency::Auto => match alpha_mode.and_then(|m| self.constant(m)) {
                Some(m) if m[0] == 0.0 => Transparency::Opaque,
                Some(m) if m[0] == 1.0 => Transparency::Scissor,
                _ => Transparency::Blend,
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
    b.alpha("alpha", Some("alpha_mode"), Some("alpha_cutoff"));
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
    b.alpha("opacity", None, None);
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
    b.alpha("geometry_opacity", None, None);
    b.drop_unless(&[("base_diffuse_roughness", 0.0), ("transmission_weight", 0.0), ("thin_film_weight", 0.0)]);
    b.cull(Some("geometry_thin_walled"));
}

fn surface_unlit(b: &mut Builder) {
    b.out.render_modes.push("unshaded");
    let albedo = format!("{} * {}", b.s("emission", Type::Float, 1.0), b.s("emission_color", Type::Color3, 1.0));
    b.set("ALBEDO", albedo);
    b.alpha("opacity", None, None);
    b.drop_unless(&[("transmission", 0.0)]);
    b.cull(None);
}
