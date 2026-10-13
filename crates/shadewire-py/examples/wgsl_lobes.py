"""A MaterialX material with a closure tree (lobes) as a complete WGSL fragment shader.

    python wgsl_lobes.py material.mtlx [--material NAME] [--hlsl out.hlsl]

Shadewire writes `fn shadewire_eval_surface() -> Surface`: the values of the material's lobes, computed from its
pattern graph. A host adds three things around it:

1. the host functions the generated code calls (`shadewire_host_normal_world()`, texture coordinates...),
   listed in the reflection's `context`;
2. the lobes and the closure tree over them: Shadewire says *which* lobes, in what tree, with which values;
   the host decides how each lobe scatters light (here: one directional light, a forward shader);
3. an entry point that sets the host values, calls `shadewire_eval_surface`, and shades.

`shadewire.wgsl.link` joins the pieces and validates them with naga, so a mistake shows as an error with its line.
"""

import argparse
import sys

import shadewire
from shadewire import mtlx, wgsl

WGSL_TYPE = {
    "float": "f32", "integer": "i32", "boolean": "bool",
    "vector2": "vec2<f32>", "vector3": "vec3<f32>", "color3": "vec3<f32>",
    "vector4": "vec4<f32>", "color4": "vec4<f32>",
}

# ------------------------------------------------------------------------------------------- part 1: host functions

# What the fragment gives each context read, by reflection name.
FRAGMENT_VALUE = {
    "position_world": "frag.position_world",
    "normal_world": "frag.normal_world",
    "tangent_world": "frag.tangent_world",
    "bitangent_world": "cross(frag.normal_world, frag.tangent_world)",
    "texcoord_0": "frag.uv",
    "view_direction_world": "normalize(frag.position_world - scene.camera_position)",
}


def host_functions(reflection):
    """One private variable and one function per context read; `host_set_context` fills them per fragment."""
    context = reflection["context"]
    out, sets = [], []
    for entry in context["varyings"] + context["derived"] + context["globals"]:
        ty = WGSL_TYPE[entry["type"]]
        var = f"host_{entry['name']}"
        out.append(f"var<private> {var}: {ty};")
        out.append(f"fn shadewire_host_{entry['name']}() -> {ty} {{ return {var}; }}")
        if entry["name"] in FRAGMENT_VALUE:
            sets.append(f"    {var} = {FRAGMENT_VALUE[entry['name']]};")
    for g in context["geomprops"]:
        ty = WGSL_TYPE[g["type"]]
        out.append(f"fn {g['function']}() -> {ty} {{ return {ty}(); }}")
    for t in context["transforms"]:
        # One object, placed at the origin unrotated: object and world space are the same here.
        out.append(f"fn shadewire_host_{t}(v: vec3<f32>) -> vec3<f32> {{ return v; }}")
    out.append("fn host_set_context(frag: Fragment) {")
    out += sets
    out.append("}")
    return "\n".join(out)


# ------------------------------------------------------------------------------------------- part 2: the lobes

# How each lobe scatters the light: (light response, top albedo for layering). `$name` is the lobe's input.
LOBE_CODE = {
    "oren_nayar_diffuse_bsdf": ("$weight * $color * diffuse(L, $normal)", "$weight * $color"),
    "burley_diffuse_bsdf": ("$weight * $color * diffuse(L, $normal)", "$weight * $color"),
    "subsurface_bsdf": ("$weight * $color * diffuse(L, $normal)", "$weight * $color"),
    "translucent_bsdf": ("vec3<f32>(0.0)", "vec3<f32>(0.0)"),
    "sheen_bsdf": ("$weight * $color * sheen(L, V, $normal, $roughness)", "$weight * $color * 0.2"),
    "dielectric_bsdf": ("$weight * $tint * dielectric(L, V, $normal, $ior, $roughness)",
                        "$weight * $tint * fresnel_ior(dot($normal, V), $ior)"),
    "conductor_bsdf": ("$weight * conductor(L, V, $normal, $ior, $extinction, $roughness)",
                       "$weight * conductor_f0($ior, $extinction)"),
    "generalized_schlick_bsdf": ("$weight * schlick(L, V, $normal, $color0, $color90, $exponent, $roughness)",
                                 "$weight * $color0"),
}

LOBE_LIBRARY = """
// L: toward the light, V: toward the viewer, n: the lobe's normal input (where a normal map arrives).
const PI: f32 = 3.14159265;

fn ggx(L: vec3<f32>, V: vec3<f32>, n_in: vec3<f32>, alpha2: vec2<f32>) -> f32 {
    let n = normalize(n_in);
    let a = max(0.5 * (alpha2.x + alpha2.y), 1e-3);
    let H = normalize(L + V);
    let nh = max(dot(n, H), 0.0);
    let nl = max(dot(n, L), 0.0);
    let nv = max(dot(n, V), 1e-4);
    let d = a * a / (PI * pow(nh * nh * (a * a - 1.0) + 1.0, 2.0));
    let k = a * 0.5;
    let g = nl / (nl * (1.0 - k) + k) * nv / (nv * (1.0 - k) + k);
    return d * g / (4.0 * nv);  // times cos(L): the light's projected solid angle
}
fn fresnel_ior(c: f32, ior: f32) -> f32 {
    let f0 = pow((ior - 1.0) / (ior + 1.0), 2.0);
    return f0 + (1.0 - f0) * pow(1.0 - clamp(c, 0.0, 1.0), 5.0);
}
fn diffuse(L: vec3<f32>, n: vec3<f32>) -> f32 { return max(dot(normalize(n), L), 0.0) / PI; }
fn sheen(L: vec3<f32>, V: vec3<f32>, n: vec3<f32>, r: f32) -> f32 {
    return max(dot(normalize(n), L), 0.0) * 0.25 * pow(1.0 - max(dot(normalize(n), V), 0.0), 2.0 / max(r, 0.05));
}
fn dielectric(L: vec3<f32>, V: vec3<f32>, n: vec3<f32>, ior: f32, alpha: vec2<f32>) -> f32 {
    return ggx(L, V, n, alpha) * fresnel_ior(dot(normalize(L + V), V), ior);
}
fn conductor_f0(ior: vec3<f32>, k: vec3<f32>) -> vec3<f32> {
    return ((ior - 1.0) * (ior - 1.0) + k * k) / ((ior + 1.0) * (ior + 1.0) + k * k);
}
fn conductor(L: vec3<f32>, V: vec3<f32>, n: vec3<f32>, ior: vec3<f32>, k: vec3<f32>, alpha: vec2<f32>) -> vec3<f32> {
    let f0 = conductor_f0(ior, k);
    let c = dot(normalize(L + V), V);
    return ggx(L, V, n, alpha) * (f0 + (1.0 - f0) * pow(1.0 - c, 5.0));
}
fn schlick(L: vec3<f32>, V: vec3<f32>, n: vec3<f32>, c0: vec3<f32>, c90: vec3<f32>, e: f32,
           alpha: vec2<f32>) -> vec3<f32> {
    return ggx(L, V, n, alpha) * mix(c0, c90, pow(1.0 - dot(normalize(L + V), V), e));
}
"""


class ClosureWriter:
    """WGSL that evaluates a closure tree for one light: `host_bsdf(s, L, V, N)`, `host_albedo(s, V, N)` and
    `host_emission(s, V, N)`."""

    def __init__(self, closure, model_struct):
        self.closure = closure
        self.model_struct = model_struct
        self.unsupported = set()

    def field(self, name):
        return f"s.{name}"

    def lobe(self, node):
        """The lobe's (response, albedo) expressions, its inputs substituted (no normal input: the geometric N)."""
        code = LOBE_CODE.get(node["kind"])
        if code is None:
            self.unsupported.add(node["kind"])
            return "vec3<f32>(0.0)", "vec3<f32>(0.0)"
        response, albedo = code
        if "normal" not in node["inputs"]:
            response, albedo = response.replace("$normal", "N"), albedo.replace("$normal", "N")
        for key, field in sorted(node["inputs"].items(), key=lambda kv: -len(kv[0])):
            response = response.replace(f"${key}", self.field(field))
            albedo = albedo.replace(f"${key}", self.field(field))
        return response, albedo

    def bsdf(self, index):
        """(response, albedo) of closure node `index`, as WGSL expressions."""
        node = self.closure["nodes"][index]
        op = node["op"]
        if op == "lobe":
            return self.lobe(node)
        if op == "layer":
            top, top_albedo = self.bsdf(node["top"])
            base, base_albedo = self.bsdf(node["base"])
            # The base sees what the top does not reflect.
            rest = f"max(vec3<f32>(1.0) - ({top_albedo}), vec3<f32>(0.0))"
            return f"({top} + {rest} * ({base}))", f"({top_albedo} + {rest} * ({base_albedo}))"
        if op == "mix":
            fg, fg_albedo = self.bsdf(node["fg"])
            bg, bg_albedo = self.bsdf(node["bg"])
            m = self.field(node["mix"])
            return f"mix({bg}, {fg}, {m})", f"mix({bg_albedo}, {fg_albedo}, {m})"
        if op == "add":
            a, a_albedo = self.bsdf(node["in1"])
            b, b_albedo = self.bsdf(node["in2"])
            return f"({a} + {b})", f"({a_albedo} + {b_albedo})"
        if op == "multiply":
            c, c_albedo = self.bsdf(node["in1"])
            w = f"vec3<f32>({self.field(node['in2'])})"
            return f"({c} * {w})", f"({c_albedo} * {w})"
        raise ValueError(f"unknown closure op {op}")

    def emission(self, index):
        node = self.closure["nodes"][index]
        op = node["op"]
        if op == "lobe":
            inputs = node["inputs"]
            if node["kind"] == "uniform_edf":
                return self.field(inputs["color"])
            if node["kind"] == "generalized_schlick_edf":
                base = self.emission(node["closures"]["base"]) if "base" in node.get("closures", {}) else "vec3<f32>(0.0)"
                c0, c90, e = (self.field(inputs[k]) for k in ("color0", "color90", "exponent"))
                return f"({base} * mix({c0}, {c90}, pow(1.0 - max(dot(N, V), 0.0), {e})))"
            self.unsupported.add(node["kind"])
            return "vec3<f32>(0.0)"
        if op == "mix":
            return f"mix({self.emission(node['bg'])}, {self.emission(node['fg'])}, {self.field(node['mix'])})"
        if op in ("add", "layer"):
            a, b = ("in1", "in2") if op == "add" else ("top", "base")
            return f"({self.emission(node[a])} + {self.emission(node[b])})"
        return f"({self.emission(node['in1'])} * vec3<f32>({self.field(node['in2'])}))"

    def write(self):
        zero = ("vec3<f32>(0.0)", "vec3<f32>(0.0)")
        bsdf, albedo = self.bsdf(self.closure["bsdf"]) if self.closure.get("bsdf") is not None else zero
        edf = self.emission(self.closure["edf"]) if self.closure.get("edf") is not None else "vec3<f32>(0.0)"
        return (
            f"fn host_bsdf(s: {self.model_struct}, L: vec3<f32>, V: vec3<f32>, N: vec3<f32>) -> vec3<f32> {{\n"
            f"    return {bsdf};\n}}\n"
            f"fn host_albedo(s: {self.model_struct}, V: vec3<f32>, N: vec3<f32>) -> vec3<f32> {{\n"
            f"    return {albedo};\n}}\n"
            f"fn host_emission(s: {self.model_struct}, V: vec3<f32>, N: vec3<f32>) -> vec3<f32> {{\n"
            f"    return {edf};\n}}\n"
        )


# ------------------------------------------------------------------------------------------- part 3: the entry point

ENTRY = """
struct Fragment {
    @location(0) position_world: vec3<f32>,
    @location(1) normal_world: vec3<f32>,
    @location(2) tangent_world: vec3<f32>,
    @location(3) uv: vec2<f32>,
}
struct Scene {
    camera_position: vec3<f32>,
    light_direction: vec3<f32>,  // toward the light
    light_color: vec3<f32>,
    ambient: vec3<f32>,
}
@group(0) @binding(0) var<uniform> scene: Scene;

@fragment
fn fs_main(frag: Fragment) -> @location(0) vec4<f32> {
    host_set_context(frag);
    let s = shadewire_eval_surface();
    let N = normalize(frag.normal_world);
    let V = normalize(scene.camera_position - frag.position_world);
    let L = normalize(scene.light_direction);
    // The light through the lobes; uniform ambient light reflected by their albedo.
    let lit = host_bsdf(s, L, V, N) * scene.light_color + host_albedo(s, V, N) * scene.ambient;
    return vec4<f32>(lit + host_emission(s, V, N), 1.0);
}
"""


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("mtlx")
    parser.add_argument("--material")
    parser.add_argument("--hlsl", help="also write the linked shader as HLSL")
    args = parser.parse_args()

    text = open(args.mtlx, encoding="utf-8").read()
    # Front end: accept the closure model, so raw BSDF graphs and named models both arrive as lobes.
    ir = mtlx.to_ir(text, path=args.mtlx, material=args.material, models=["surface"], values_as_uniforms=False)
    print(ir, "->", ir.optimize())
    # Back end: the material's own resources (uniforms, textures) in group 1; the host owns group 0.
    compiled = wgsl.compile(ir, group=1)
    reflection = compiled.reflection
    print(f"generated: {len(compiled.wgsl.splitlines())} lines, struct {compiled.model_struct}, "
          f"{len(reflection['textures'])} textures, {len(reflection['uniforms'])} uniforms")
    print("closure tree:", [n.get("kind", n["op"]) for n in reflection["closure"]["nodes"]])

    writer = ClosureWriter(reflection["closure"], compiled.model_struct)
    host = "\n".join([host_functions(reflection), LOBE_LIBRARY, writer.write(), ENTRY])
    if writer.unsupported:
        print("lobes this host does not shade:", ", ".join(sorted(writer.unsupported)))
    try:
        linked = wgsl.link(compiled.wgsl, host)
    except shadewire.ShadewireError as e:
        sys.exit(f"the host does not link:\n{e}")
    print(f"linked and validated: {len(linked.splitlines())} lines")
    if args.hlsl:
        open(args.hlsl, "w", encoding="utf-8").write(wgsl.translate(linked, "hlsl", 60))
        print("wrote", args.hlsl)


if __name__ == "__main__":
    main()
