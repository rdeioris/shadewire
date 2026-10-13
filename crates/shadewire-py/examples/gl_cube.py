"""A spinning cube with a MaterialX material in a pygame window, drawn with OpenGL. One file, nothing else needed.

    pip install shadewire pygame PyOpenGL pillow
    python gl_cube.py material.mtlx [--material NAME] [--size 800] [--frames N --screenshot out.png]

Shadewire compiles the material to WGSL: `shadewire_eval_surface()` computes the values of its lobes, and the
reflection lists the host functions the code calls, the closure tree over the lobes, and the resources to bind.
This file is the host:

1. the host functions (`shadewire_host_normal_world()`...), filled from the vertex shader's outputs;
2. the lobes and the tree over them, written as WGSL from the reflection, for one directional light;
3. a vertex and a fragment entry point.

`shadewire.wgsl.link` validates the whole module, and `shadewire.wgsl.translate` writes each stage as GLSL 4.20
core. GLSL has no bind groups: `@group(g) @binding(b)` becomes binding point `32 * g + b`, so the scene's
uniform blocks sit at 0 and 1 and the material's resources (group 1) from 32 on; each texture combines with its
sampler into one `sampler2D` at the texture's binding point. Escape or closing the window quits.
"""

import argparse
import ctypes
import math
import os
import struct
import sys

import pygame
from OpenGL import GL
from OpenGL.GL.shaders import compileProgram, compileShader
from PIL import Image

import shadewire
from shadewire import mtlx, wgsl

MATERIAL_GROUP = 1  # the material's uniform block, textures and samplers; the scene owns group 0
GLSL_VERSION = 420

WGSL_TYPE = {
    "float": "f32", "integer": "i32", "boolean": "bool",
    "vector2": "vec2<f32>", "vector3": "vec3<f32>", "color3": "vec3<f32>",
    "vector4": "vec4<f32>", "color4": "vec4<f32>",
}

# ---------------------------------------------------------------------------------------------------- 1. host functions

# Where the fragment keeps each context read, by reflection name.
FRAGMENT_VALUE = {
    "position_world": "frag.position_world",
    "normal_world": "frag.normal_world",
    "tangent_world": "frag.tangent_world",
    "bitangent_world": "cross(frag.normal_world, frag.tangent_world)",
    "texcoord_0": "frag.uv",
    "view_direction_world": "normalize(frag.position_world - scene.camera_position)",
}


def host_functions(reflection):
    """One private variable and one function per context read the material makes; `host_set_context` fills
    them per fragment. Reads the fragment does not give stay zero."""
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
        # The cube sits at the origin unrotated: object and world space are the same.
        out.append(f"fn shadewire_host_{t}(v: vec3<f32>) -> vec3<f32> {{ return v; }}")
    out.append("fn host_set_context(frag: Fragment) {")
    out += sets
    out.append("}")
    return "\n".join(out)


# ---------------------------------------------------------------------------------------------------- 2. the lobes

# How each lobe answers one light: (response to the light, albedo: what it reflects of uniform light, and what a
# layer over it takes away). `$name` is the lobe's input; a lobe without a normal input uses the geometric N.
LOBE_CODE = {
    "oren_nayar_diffuse_bsdf": ("$weight * $color * diffuse(L, $normal)", "$weight * $color"),
    "burley_diffuse_bsdf": ("$weight * $color * diffuse(L, $normal)", "$weight * $color"),
    "subsurface_bsdf": ("$weight * $color * diffuse(L, $normal)", "$weight * $color"),
    "translucent_bsdf": ("vec3<f32>(0.0)", "vec3<f32>(0.0)"),
    "sheen_bsdf": ("$weight * $color * sheen(L, $normal, $roughness)", "$weight * $color * 0.2"),
    "dielectric_bsdf": ("$weight * $tint * dielectric(L, $normal, $ior, $roughness)",
                        "$weight * $tint * fresnel_ior(dot($normal, V), $ior)"),
    "conductor_bsdf": ("$weight * conductor(L, $normal, $ior, $extinction, $roughness)",
                       "$weight * conductor_f0($ior, $extinction)"),
    "generalized_schlick_bsdf": ("$weight * schlick(L, $normal, $color0, $color90, $exponent, $roughness)",
                                 "$weight * $color0"),
}

LOBE_LIBRARY = """
const PI: f32 = 3.14159265;
// The shading point: the geometric normal and the direction to the viewer.
var<private> N: vec3<f32>;
var<private> V: vec3<f32>;

fn ggx(L: vec3<f32>, n_in: vec3<f32>, alpha2: vec2<f32>) -> f32 {
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
fn sheen(L: vec3<f32>, n: vec3<f32>, r: f32) -> f32 {
    return max(dot(normalize(n), L), 0.0) * 0.25 * pow(1.0 - max(dot(normalize(n), V), 0.0), 2.0 / max(r, 0.05));
}
fn dielectric(L: vec3<f32>, n: vec3<f32>, ior: f32, alpha: vec2<f32>) -> f32 {
    return ggx(L, n, alpha) * fresnel_ior(dot(normalize(L + V), V), ior);
}
fn conductor_f0(ior: vec3<f32>, k: vec3<f32>) -> vec3<f32> {
    return ((ior - 1.0) * (ior - 1.0) + k * k) / ((ior + 1.0) * (ior + 1.0) + k * k);
}
fn conductor(L: vec3<f32>, n: vec3<f32>, ior: vec3<f32>, k: vec3<f32>, alpha: vec2<f32>) -> vec3<f32> {
    let f0 = conductor_f0(ior, k);
    return ggx(L, n, alpha) * (f0 + (1.0 - f0) * pow(1.0 - dot(normalize(L + V), V), 5.0));
}
fn schlick(L: vec3<f32>, n: vec3<f32>, c0: vec3<f32>, c90: vec3<f32>, e: f32, alpha: vec2<f32>) -> vec3<f32> {
    return ggx(L, n, alpha) * mix(c0, c90, pow(1.0 - dot(normalize(L + V), V), e));
}
"""

ZERO = "vec3<f32>(0.0)"


def closure_code(closure, model_struct):
    """`host_bsdf(s, L)`, `host_albedo(s)` and `host_emission(s)` for the material's closure tree, and the lobes
    this host does not shade."""
    nodes = closure["nodes"]
    unsupported = set()

    def lobe(node):
        code = LOBE_CODE.get(node["kind"])
        if code is None:
            unsupported.add(node["kind"])
            return ZERO, ZERO
        response, albedo = code
        if "normal" not in node["inputs"]:
            response, albedo = response.replace("$normal", "N"), albedo.replace("$normal", "N")
        for key, field in sorted(node["inputs"].items(), key=lambda kv: -len(kv[0])):
            response, albedo = response.replace(f"${key}", f"s.{field}"), albedo.replace(f"${key}", f"s.{field}")
        return response, albedo

    def layer(node):
        top, top_albedo = bsdf(node["top"])
        base, base_albedo = bsdf(node["base"])
        rest = f"max(vec3<f32>(1.0) - ({top_albedo}), vec3<f32>(0.0))"  # what the top lets through to the base
        return f"({top} + {rest} * ({base}))", f"({top_albedo} + {rest} * ({base_albedo}))"

    def mix(node):
        (fg, fg_albedo), (bg, bg_albedo), m = bsdf(node["fg"]), bsdf(node["bg"]), f"s.{node['mix']}"
        return f"mix({bg}, {fg}, {m})", f"mix({bg_albedo}, {fg_albedo}, {m})"

    def add(node):
        (a, a_albedo), (b, b_albedo) = bsdf(node["in1"]), bsdf(node["in2"])
        return f"({a} + {b})", f"({a_albedo} + {b_albedo})"

    def multiply(node):
        (c, c_albedo), w = bsdf(node["in1"]), f"vec3<f32>(s.{node['in2']})"
        return f"({c} * {w})", f"({c_albedo} * {w})"

    closure_ops = {"lobe": lobe, "layer": layer, "mix": mix, "add": add, "multiply": multiply}

    def bsdf(index):
        node = nodes[index]
        return closure_ops[node["op"]](node)

    def emitter(node):
        inputs = node["inputs"]
        if node["kind"] == "uniform_edf":
            return f"s.{inputs['color']}"
        if node["kind"] == "generalized_schlick_edf":
            base = emission(node["closures"]["base"]) if "base" in node.get("closures", {}) else ZERO
            c0, c90, e = (f"s.{inputs[k]}" for k in ("color0", "color90", "exponent"))
            return f"({base} * mix({c0}, {c90}, pow(1.0 - max(dot(N, V), 0.0), {e})))"
        unsupported.add(node["kind"])
        return ZERO

    emission_ops = {
        "lobe": emitter,
        "mix": lambda n: f"mix({emission(n['bg'])}, {emission(n['fg'])}, s.{n['mix']})",
        "add": lambda n: f"({emission(n['in1'])} + {emission(n['in2'])})",
        "layer": lambda n: f"({emission(n['top'])} + {emission(n['base'])})",
        "multiply": lambda n: f"({emission(n['in1'])} * vec3<f32>(s.{n['in2']}))",
    }

    def emission(index):
        node = nodes[index]
        return emission_ops[node["op"]](node)

    response, albedo = bsdf(closure["bsdf"]) if closure.get("bsdf") is not None else (ZERO, ZERO)
    emitted = emission(closure["edf"]) if closure.get("edf") is not None else ZERO
    code = (
        f"fn host_bsdf(s: {model_struct}, L: vec3<f32>) -> vec3<f32> {{\n    return {response};\n}}\n"
        f"fn host_albedo(s: {model_struct}) -> vec3<f32> {{\n    return {albedo};\n}}\n"
        f"fn host_emission(s: {model_struct}) -> vec3<f32> {{\n    return {emitted};\n}}\n"
    )
    return code, unsupported


# ---------------------------------------------------------------------------------------------------- 3. entry points

ENTRY_POINTS = """
struct Scene {
    camera_position: vec3<f32>,
    light_direction: vec3<f32>,  // toward the light
    light_color: vec3<f32>,
    ambient: vec3<f32>,
}
struct Camera {
    view_proj: mat4x4<f32>,
}
@group(0) @binding(0) var<uniform> scene: Scene;
@group(0) @binding(1) var<uniform> camera: Camera;

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec3<f32>,
    @location(3) uv: vec2<f32>,
}
struct Fragment {
    @builtin(position) clip: vec4<f32>,
    @location(0) position_world: vec3<f32>,
    @location(1) normal_world: vec3<f32>,
    @location(2) tangent_world: vec3<f32>,
    @location(3) uv: vec2<f32>,
}

@vertex
fn vs_main(v: Vertex) -> Fragment {
    // The cube sits at the origin unrotated: object space is world space.
    var out: Fragment;
    out.clip = camera.view_proj * vec4<f32>(v.position, 1.0);
    out.position_world = v.position;
    out.normal_world = v.normal;
    out.tangent_world = v.tangent;
    out.uv = v.uv;
    return out;
}

@fragment
fn fs_main(frag: Fragment) -> @location(0) vec4<f32> {
    host_set_context(frag);
    let s = shadewire_eval_surface();
    N = normalize(frag.normal_world);
    V = normalize(scene.camera_position - frag.position_world);
    // The light through the lobes; uniform ambient light reflected by their albedo.
    let lit = host_bsdf(s, normalize(scene.light_direction)) * scene.light_color + host_albedo(s) * scene.ambient;
    return vec4<f32>(lit + host_emission(s), 1.0);  // linear: GL_FRAMEBUFFER_SRGB encodes it
}
"""


def glsl_program(mtlx_path, material):
    """The material compiled, joined with the host, validated, and written as a GLSL program; the reflection."""
    text = open(mtlx_path, encoding="utf-8").read()
    ir = mtlx.to_ir(text, path=mtlx_path, material=material, models=["surface"])
    compiled = wgsl.compile(ir, group=MATERIAL_GROUP)
    reflection = compiled.reflection
    closures, unsupported = closure_code(reflection["closure"], compiled.model_struct)
    if unsupported:
        print("lobes this host does not shade:", ", ".join(sorted(unsupported)))
    print(f"{ir.model}: {len(reflection['closure']['nodes'])} closure nodes, "
          f"{len(reflection['uniforms'])} uniforms, {len(reflection['textures'])} textures")
    linked = wgsl.link(compiled.wgsl, "\n".join([host_functions(reflection), LOBE_LIBRARY, closures, ENTRY_POINTS]))
    stages = [compileShader(wgsl.translate(linked, "glsl", GLSL_VERSION, entry_point=entry), kind)
              for entry, kind in (("vs_main", GL.GL_VERTEX_SHADER), ("fs_main", GL.GL_FRAGMENT_SHADER))]
    return compileProgram(*stages, validate=False), reflection


# ---------------------------------------------------------------------------------------------------- the scene


def normalize(v):
    n = math.sqrt(sum(x * x for x in v))
    return [x / n for x in v]


def cross(a, b):
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]


def dot(a, b):
    return sum(x * y for x, y in zip(a, b))


def view_proj(eye, fov_y=math.radians(35), near=0.1, far=100.0):
    """Perspective (WebGPU's 0..1 depth: Shadewire's GLSL maps it to OpenGL's) times a look-at the origin, Y up.
    Column-major, as GLSL's `mat4` and WGSL's `mat4x4`."""
    f = normalize([-e for e in eye])
    s = normalize(cross(f, [0, 1, 0]))
    u = cross(s, f)
    view = [s[0], u[0], -f[0], 0, s[1], u[1], -f[1], 0, s[2], u[2], -f[2], 0, -dot(s, eye), -dot(u, eye), dot(f, eye), 1]
    t = 1.0 / math.tan(fov_y / 2.0)
    proj = [t, 0, 0, 0, 0, t, 0, 0, 0, 0, far / (near - far), -1, 0, 0, near * far / (near - far), 0]
    return [sum(proj[k * 4 + r] * view[c * 4 + k] for k in range(4)) for c in range(4) for r in range(4)]


def scene_blocks(angle):
    """The `Scene` and `Camera` uniform blocks for the camera at `angle` around the cube (std140: each vec3
    padded to 16 bytes, as WGSL's uniform layout)."""
    eye = [5.6 * math.sin(angle), 2.4, 5.6 * math.cos(angle)]
    light = normalize([0.6, 1.0, 0.8])
    scene = struct.pack("<16f", *eye, 0, *light, 0, 3.0, 2.9, 2.7, 0, 0.35, 0.38, 0.45, 0)
    return scene, struct.pack("<16f", *view_proj(eye))


def cube_vao():
    """Six faces of 2 x 2 units, each with its own normal, tangent (along +U) and UVs 0..1 (V up, as MaterialX)."""
    faces = [  # (normal, tangent along U, bitangent along V)
        ((0, 0, 1), (1, 0, 0), (0, 1, 0)), ((0, 0, -1), (-1, 0, 0), (0, 1, 0)),
        ((1, 0, 0), (0, 0, -1), (0, 1, 0)), ((-1, 0, 0), (0, 0, 1), (0, 1, 0)),
        ((0, 1, 0), (1, 0, 0), (0, 0, -1)), ((0, -1, 0), (1, 0, 0), (0, 0, 1)),
    ]
    vertices, indices = [], []
    for n, t, b in faces:
        base = len(vertices) // 11
        for u, v in ((0, 0), (1, 0), (1, 1), (0, 1)):
            vertices += [*(n[k] + t[k] * (2 * u - 1) + b[k] * (2 * v - 1) for k in range(3)), *n, *t, u, v]
        indices += [base, base + 1, base + 2, base, base + 2, base + 3]
    vao = GL.glGenVertexArrays(1)
    GL.glBindVertexArray(vao)
    GL.glBindBuffer(GL.GL_ARRAY_BUFFER, GL.glGenBuffers(1))
    data = struct.pack(f"{len(vertices)}f", *vertices)
    GL.glBufferData(GL.GL_ARRAY_BUFFER, len(data), data, GL.GL_STATIC_DRAW)
    GL.glBindBuffer(GL.GL_ELEMENT_ARRAY_BUFFER, GL.glGenBuffers(1))
    data = struct.pack(f"{len(indices)}I", *indices)
    GL.glBufferData(GL.GL_ELEMENT_ARRAY_BUFFER, len(data), data, GL.GL_STATIC_DRAW)
    for location, floats, offset in ((0, 3, 0), (1, 3, 12), (2, 3, 24), (3, 2, 36)):
        GL.glEnableVertexAttribArray(location)
        GL.glVertexAttribPointer(location, floats, GL.GL_FLOAT, GL.GL_FALSE, 44, ctypes.c_void_p(offset))
    return vao, len(indices)


# ---------------------------------------------------------------------------------------------------- the material


def gl_binding(group, binding):
    """Where Shadewire's GLSL puts WGSL `@group(group) @binding(binding)`."""
    return 32 * group + binding


def uniform_buffer(data, point):
    buffer = GL.glGenBuffers(1)
    GL.glBindBuffer(GL.GL_UNIFORM_BUFFER, buffer)
    GL.glBufferData(GL.GL_UNIFORM_BUFFER, len(data), data, GL.GL_DYNAMIC_DRAW)
    GL.glBindBufferBase(GL.GL_UNIFORM_BUFFER, point, buffer)
    return buffer


def material_uniforms(reflection):
    """The material's uniform block: each uniform's default at its offset (booleans as u32, integers as i32)."""
    data = bytearray(max(reflection["uniform_block_size"], 16))
    for u in reflection["uniforms"]:
        values = u["default"] if isinstance(u["default"], list) else [u["default"]]
        for k, x in enumerate(values):
            if isinstance(x, bool):
                struct.pack_into("<I", data, u["offset"] + 4 * k, int(x))
            elif isinstance(x, int) and u["type"] == "integer":
                struct.pack_into("<i", data, u["offset"] + 4 * k, x)
            else:
                struct.pack_into("<f", data, u["offset"] + 4 * k, float(x))
    return bytes(data)


WRAP = {"periodic": GL.GL_REPEAT, "mirror": GL.GL_MIRRORED_REPEAT, "clamp": GL.GL_CLAMP_TO_EDGE,
        "constant": GL.GL_CLAMP_TO_EDGE}


def bind_material(reflection, base_dir):
    """The material's uniform block and textures at the binding points of group `MATERIAL_GROUP`."""
    if reflection["uniform_binding"] is not None:
        uniform_buffer(material_uniforms(reflection), gl_binding(MATERIAL_GROUP, reflection["uniform_binding"]))
    for t in reflection["textures"]:
        path = os.path.join(base_dir, t["file"])
        try:
            image = Image.open(path).convert("RGBA")
        except OSError:
            print(f"warning: texture {path} not found; it reads as mid grey")
            image = Image.new("RGBA", (1, 1), (128, 128, 128, 255))
        GL.glActiveTexture(GL.GL_TEXTURE0 + gl_binding(MATERIAL_GROUP, t["binding"]))
        GL.glBindTexture(GL.GL_TEXTURE_2D, GL.glGenTextures(1))
        # Raw texels (GL_RGBA8, never sRGB: the generated code converts colorspaces), top row first: Shadewire
        # flips V when sampling.
        GL.glTexImage2D(GL.GL_TEXTURE_2D, 0, GL.GL_RGBA8, image.width, image.height, 0, GL.GL_RGBA,
                        GL.GL_UNSIGNED_BYTE, image.tobytes())
        GL.glGenerateMipmap(GL.GL_TEXTURE_2D)
        GL.glTexParameteri(GL.GL_TEXTURE_2D, GL.GL_TEXTURE_WRAP_S, WRAP[t["address"][0]])
        GL.glTexParameteri(GL.GL_TEXTURE_2D, GL.GL_TEXTURE_WRAP_T, WRAP[t["address"][1]])
        nearest = t["filter"] == "closest"
        GL.glTexParameteri(GL.GL_TEXTURE_2D, GL.GL_TEXTURE_MAG_FILTER, GL.GL_NEAREST if nearest else GL.GL_LINEAR)
        GL.glTexParameteri(GL.GL_TEXTURE_2D, GL.GL_TEXTURE_MIN_FILTER,
                           GL.GL_NEAREST_MIPMAP_NEAREST if nearest else GL.GL_LINEAR_MIPMAP_LINEAR)


# ---------------------------------------------------------------------------------------------------- the window


def open_window(size):
    pygame.init()
    for attribute, value in ((pygame.GL_CONTEXT_MAJOR_VERSION, 4), (pygame.GL_CONTEXT_MINOR_VERSION, 2),
                             (pygame.GL_CONTEXT_PROFILE_MASK, pygame.GL_CONTEXT_PROFILE_CORE),
                             (pygame.GL_MULTISAMPLEBUFFERS, 1), (pygame.GL_MULTISAMPLESAMPLES, 4),
                             (pygame.GL_FRAMEBUFFER_SRGB_CAPABLE, 1), (pygame.GL_DEPTH_SIZE, 24)):
        pygame.display.gl_set_attribute(attribute, value)
    pygame.display.set_mode((size, size), pygame.OPENGL | pygame.DOUBLEBUF)
    pygame.display.set_caption("Shadewire MaterialX cube (OpenGL)")
    print("OpenGL", GL.glGetString(GL.GL_VERSION).decode(), "on", GL.glGetString(GL.GL_RENDERER).decode())


def screenshot(size, path):
    GL.glReadBuffer(GL.GL_BACK)
    pixels = GL.glReadPixels(0, 0, size, size, GL.GL_RGB, GL.GL_UNSIGNED_BYTE)
    # OpenGL's rows start at the bottom.
    Image.frombytes("RGB", (size, size), pixels).transpose(Image.Transpose.FLIP_TOP_BOTTOM).save(path)
    print("wrote", path)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("mtlx")
    parser.add_argument("--material")
    parser.add_argument("--size", type=int, default=800)
    parser.add_argument("--frames", type=int, help="draw this many frames, then quit")
    parser.add_argument("--screenshot", help="save the last frame here")
    args = parser.parse_args()

    open_window(args.size)
    try:
        program, reflection = glsl_program(args.mtlx, args.material)
    except shadewire.ShadewireError as e:
        sys.exit(f"shadewire: {e}")
    vao, index_count = cube_vao()
    angle = 0.6
    scene, camera = scene_blocks(angle)
    scene_buffer, camera_buffer = uniform_buffer(scene, gl_binding(0, 0)), uniform_buffer(camera, gl_binding(0, 1))
    bind_material(reflection, os.path.dirname(args.mtlx))

    GL.glEnable(GL.GL_DEPTH_TEST)
    GL.glEnable(GL.GL_CULL_FACE)
    GL.glEnable(GL.GL_MULTISAMPLE)
    GL.glEnable(GL.GL_FRAMEBUFFER_SRGB)
    clock = pygame.time.Clock()
    frame = 0
    running = True
    while running:
        for event in pygame.event.get():
            if event.type == pygame.QUIT or (event.type == pygame.KEYDOWN and event.key == pygame.K_ESCAPE):
                running = False
        angle += clock.tick(60) / 1000.0 * 0.5
        for buffer, data in zip((scene_buffer, camera_buffer), scene_blocks(angle)):
            GL.glBindBuffer(GL.GL_UNIFORM_BUFFER, buffer)
            GL.glBufferSubData(GL.GL_UNIFORM_BUFFER, 0, len(data), data)
        GL.glClearColor(0.08, 0.08, 0.09, 1.0)
        GL.glClear(GL.GL_COLOR_BUFFER_BIT | GL.GL_DEPTH_BUFFER_BIT)
        GL.glUseProgram(program)
        GL.glBindVertexArray(vao)
        GL.glDrawElements(GL.GL_TRIANGLES, index_count, GL.GL_UNSIGNED_INT, None)
        frame += 1
        if args.frames and frame >= args.frames:
            if args.screenshot:
                screenshot(args.size, args.screenshot)
            running = False
        pygame.display.flip()
    pygame.quit()


if __name__ == "__main__":
    main()
