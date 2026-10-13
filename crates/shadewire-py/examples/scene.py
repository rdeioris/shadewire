"""What the cube examples share, whatever the graphics API: the mesh, the camera math, the scene and material
uniform blocks, and the shader (Shadewire's generated module plus the host of `wgsl_lobes.py` and a vertex
stage), linked and validated as WGSL."""

import math
import struct

from shadewire import mtlx, wgsl
from wgsl_lobes import ENTRY, host_functions

# The material's resources (uniform block, textures, samplers) go in group 1; the scene owns group 0
# (binding 0: `Scene` of wgsl_lobes.ENTRY, binding 1: `Camera` below).
MATERIAL_GROUP = 1

VERTEX = """
struct Camera {
    view_proj: mat4x4<f32>,
}
@group(0) @binding(1) var<uniform> camera: Camera;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec3<f32>,
    @location(3) uv: vec2<f32>,
}
struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) position_world: vec3<f32>,
    @location(1) normal_world: vec3<f32>,
    @location(2) tangent_world: vec3<f32>,
    @location(3) uv: vec2<f32>,
}

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    // The cube sits at the origin unrotated: object space is world space.
    var out: VertexOut;
    out.clip = camera.view_proj * vec4<f32>(v.position, 1.0);
    out.position_world = v.position;
    out.normal_world = v.normal;
    out.tangent_world = v.tangent;
    out.uv = v.uv;
    return out;
}
"""

# Vertex layout: position, normal, tangent (3 floats each), uv (2 floats).
VERTEX_STRIDE = 11 * 4
VERTEX_ATTRIBUTES = [(0, 3, 0), (1, 3, 12), (2, 3, 24), (3, 2, 36)]  # (location, floats, byte offset)

# ---------------------------------------------------------------------------------------------------- math


def normalize(v):
    n = math.sqrt(sum(x * x for x in v))
    return [x / n for x in v]


def cross(a, b):
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]


def dot(a, b):
    return sum(x * y for x, y in zip(a, b))


def look_at(eye, target, up):
    """A right-handed view matrix, column-major (WGSL's `mat4x4` and GLSL's `mat4` layout)."""
    f = normalize([t - e for t, e in zip(target, eye)])
    s = normalize(cross(f, up))
    u = cross(s, f)
    return [s[0], u[0], -f[0], 0.0, s[1], u[1], -f[1], 0.0, s[2], u[2], -f[2], 0.0,
            -dot(s, eye), -dot(u, eye), dot(f, eye), 1.0]


def perspective(fov_y, aspect, near, far):
    """WebGPU's clip space, depth 0..1 (Shadewire's GLSL maps it to OpenGL's -1..1). Column-major."""
    f = 1.0 / math.tan(fov_y / 2.0)
    return [f / aspect, 0, 0, 0, 0, f, 0, 0, 0, 0, far / (near - far), -1, 0, 0, near * far / (near - far), 0]


def multiply(a, b):
    """a * b for column-major 4x4 matrices."""
    return [sum(a[k * 4 + r] * b[c * 4 + k] for k in range(4)) for c in range(4) for r in range(4)]


# ---------------------------------------------------------------------------------------------------- the scene


def cube():
    """Six faces of 2 x 2 units, each with its own normal, tangent (along +U) and UVs 0..1 (V up, as MaterialX).
    Returns (vertex bytes, index bytes, index count)."""
    faces = [  # (normal, tangent along U, bitangent along V)
        ((0, 0, 1), (1, 0, 0), (0, 1, 0)), ((0, 0, -1), (-1, 0, 0), (0, 1, 0)),
        ((1, 0, 0), (0, 0, -1), (0, 1, 0)), ((-1, 0, 0), (0, 0, 1), (0, 1, 0)),
        ((0, 1, 0), (1, 0, 0), (0, 0, -1)), ((0, -1, 0), (1, 0, 0), (0, 0, 1)),
    ]
    vertices, indices = [], []
    for n, t, b in faces:
        base = len(vertices) // 11
        for u, v in ((0, 0), (1, 0), (1, 1), (0, 1)):
            p = [n[k] + t[k] * (2 * u - 1) + b[k] * (2 * v - 1) for k in range(3)]
            vertices += [*p, *n, *t, u, v]
        indices += [base, base + 1, base + 2, base, base + 2, base + 3]
    return struct.pack(f"{len(vertices)}f", *vertices), struct.pack(f"{len(indices)}I", *indices), len(indices)


def scene_bytes(eye):
    """`Scene` of wgsl_lobes.ENTRY: camera position, direction to the light, light color, ambient (vec3 each,
    padded to 16 bytes as WGSL's uniform layout and std140 both do)."""
    return struct.pack("<16f", *eye, 0, *normalize([0.6, 1.0, 0.8]), 0, 3.0, 2.9, 2.7, 0, 0.35, 0.38, 0.45, 0)


def camera_bytes(eye, aspect):
    view_proj = multiply(perspective(math.radians(35), aspect, 0.1, 100.0), look_at(eye, [0, 0, 0], [0, 1, 0]))
    return struct.pack("<16f", *view_proj)


def orbit(angle, radius=5.6, height=2.4):
    return [radius * math.sin(angle), height, radius * math.cos(angle)]


# ---------------------------------------------------------------------------------------------------- the material


def uniform_bytes(reflection):
    """The material's uniform block: each uniform's default at its offset (booleans as u32, integers as i32)."""
    data = bytearray(max(reflection["uniform_block_size"], 16))
    for u in reflection["uniforms"]:
        value = u["default"]
        values = value if isinstance(value, list) else [value]
        for k, x in enumerate(values):
            if isinstance(x, bool):
                struct.pack_into("<I", data, u["offset"] + 4 * k, int(x))
            elif isinstance(x, int) and u["type"] == "integer":
                struct.pack_into("<i", data, u["offset"] + 4 * k, x)
            else:
                struct.pack_into("<f", data, u["offset"] + 4 * k, float(x))
    return bytes(data)


def compile_material(path, material=None):
    """The material compiled to WGSL with its reflection, and the whole shader (generated module, host, vertex
    stage) linked and validated."""
    text = open(path, encoding="utf-8").read()
    ir = mtlx.to_ir(text, path=path, material=material, models=["surface"])
    compiled = wgsl.compile(ir, group=MATERIAL_GROUP)
    reflection = compiled.reflection
    host = "\n".join([host_functions(reflection), ENTRY, VERTEX])
    print(f"{ir.model}: {len(reflection['closure']['nodes'])} closure nodes, "
          f"{len(reflection['uniforms'])} uniforms, {len(reflection['textures'])} textures")
    return compiled, wgsl.link(compiled.wgsl, host)
