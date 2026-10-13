"""A MaterialX material with a closure tree (lobes) as a complete WGSL fragment shader.

    python wgsl_lobes.py material.mtlx [--material NAME] [--hlsl out.hlsl]

Shadewire writes `fn shadewire_eval_surface() -> Surface`, the values of the material's lobes computed from its
pattern graph, and the functions that evaluate its closure tree as MaterialX defines it:
`shadewire_eval_bsdf(s, l, v, n)` (the response to light from `l`, and what passes to a layer below),
`shadewire_eval_albedo(s, v, n)` (the response to uniform white light) and `shadewire_eval_emission(s, v, n)`.
A host adds two things around them:

1. the host functions the generated code calls (`shadewire_host_normal_world()`, texture coordinates...),
   listed in the reflection's `context`;
2. an entry point that sets the host values, calls `shadewire_eval_surface`, and lights it.

The host's code follows the generated code in one module, which a WGSL compiler (wgpu, naga, Tint) validates.
"""

import argparse

from shadewire import hlsl, mtlx, wgsl

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


# ------------------------------------------------------------------------------------------- part 2: the entry point

# The generated module evaluates the closure tree itself (`shadewire_eval_bsdf`, `shadewire_eval_albedo`,
# `shadewire_eval_emission`, MaterialX's lobes and combinations): the entry brings the lights.
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
    let n = normalize(frag.normal_world);
    let v = normalize(scene.camera_position - frag.position_world);
    let l = normalize(scene.light_direction);
    // The light through the lobes; uniform ambient light reflected by their albedo.
    let lit = shadewire_eval_bsdf(s, l, v, n).response * scene.light_color + shadewire_eval_albedo(s, v, n) * scene.ambient;
    return vec4<f32>(lit + shadewire_eval_emission(s, v, n), 1.0);
}
"""


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("mtlx")
    parser.add_argument("--material")
    parser.add_argument("--out", help="write the whole WGSL module here")
    parser.add_argument("--hlsl", help="also write the material as an HLSL library")
    args = parser.parse_args()

    text = open(args.mtlx, encoding="utf-8").read()
    # Front end: accept the closure model, so raw BSDF graphs and named models both arrive as lobes.
    ir = mtlx.to_ir(text, location=args.mtlx, material=args.material, models=["surface"], values_as_uniforms=False)
    print(ir, "->", ir.optimize())
    # Back end: the material's own resources (uniforms, textures) in group 1; the host owns group 0.
    compiled = wgsl.compile(ir, group=1)
    reflection = compiled.reflection
    print(f"generated: {len(compiled.code.splitlines())} lines, struct {compiled.model_struct}, "
          f"{len(reflection['textures'])} textures, {len(reflection['uniforms'])} uniforms")
    print("closure tree:", [n.get("kind", n["op"]) for n in reflection["closure"]["nodes"]])

    module = "\n".join([compiled.code, host_functions(reflection), ENTRY])
    print(f"the whole module: {len(module.splitlines())} lines")
    if args.out:
        open(args.out, "w", encoding="utf-8").write(module)
        print("wrote", args.out)
    if args.hlsl:
        open(args.hlsl, "w", encoding="utf-8").write(hlsl.compile(ir, group=1).code)
        print("wrote", args.hlsl)


if __name__ == "__main__":
    main()
