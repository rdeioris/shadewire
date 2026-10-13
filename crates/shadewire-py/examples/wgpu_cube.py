"""A cube with a MaterialX material, rendered with wgpu-py and saved as a PNG.

    pip install shadewire wgpu pillow
    python wgpu_cube.py material.mtlx [--material NAME] [--out cube.png] [--size 800]

Shadewire compiles the material: its pattern graph to `shadewire_eval_surface()`, and its lobes to a closure
tree in the reflection. This file is the host: a cube mesh, a camera, a light, the material's own uniform block
and textures bound where the reflection says, and the shading of the lobes (the host pieces of
`wgsl_lobes.py`: the context functions, the lobe library, the closure tree as WGSL).
"""

import argparse
import math
import os
import struct

import wgpu
from PIL import Image

import shadewire
from shadewire import mtlx, wgsl
from wgsl_lobes import ENTRY, LOBE_LIBRARY, ClosureWriter, host_functions

# The material's resources (uniform block, textures, samplers) go in group 1; the scene owns group 0.
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

# ---------------------------------------------------------------------------------------------------- math


def normalize(v):
    n = math.sqrt(sum(x * x for x in v))
    return [x / n for x in v]


def cross(a, b):
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]


def dot(a, b):
    return sum(x * y for x, y in zip(a, b))


def look_at(eye, target, up):
    """A right-handed view matrix, column-major (WGSL's `mat4x4` layout)."""
    f = normalize([t - e for t, e in zip(target, eye)])
    s = normalize(cross(f, up))
    u = cross(s, f)
    return [s[0], u[0], -f[0], 0.0, s[1], u[1], -f[1], 0.0, s[2], u[2], -f[2], 0.0,
            -dot(s, eye), -dot(u, eye), dot(f, eye), 1.0]


def perspective(fov_y, aspect, near, far):
    """WebGPU's clip space: depth from 0 to 1. Column-major."""
    f = 1.0 / math.tan(fov_y / 2.0)
    return [f / aspect, 0, 0, 0, 0, f, 0, 0, 0, 0, far / (near - far), -1, 0, 0, near * far / (near - far), 0]


def multiply(a, b):
    """a * b for column-major 4x4 matrices."""
    return [sum(a[k * 4 + r] * b[c * 4 + k] for k in range(4)) for c in range(4) for r in range(4)]


# ---------------------------------------------------------------------------------------------------- the cube


def cube():
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
            p = [n[k] + t[k] * (2 * u - 1) + b[k] * (2 * v - 1) for k in range(3)]
            vertices += [*p, *n, *t, u, v]
        indices += [base, base + 1, base + 2, base, base + 2, base + 3]
    return struct.pack(f"{len(vertices)}f", *vertices), struct.pack(f"{len(indices)}I", *indices), len(indices)


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


ADDRESS = {"periodic": "repeat", "mirror": "mirror-repeat", "clamp": "clamp-to-edge", "constant": "clamp-to-edge"}


def material_entries(device, reflection, base_dir):
    """Bind group entries for the material's uniform block, textures and samplers, as the reflection places them."""
    entries = []
    if reflection["uniform_binding"] is not None:
        data = uniform_bytes(reflection)
        buffer = device.create_buffer_with_data(data=data, usage=wgpu.BufferUsage.UNIFORM)
        entries.append({"binding": reflection["uniform_binding"], "resource": {"buffer": buffer, "size": len(data)}})
    for t in reflection["textures"]:
        path = os.path.join(base_dir, t["file"])
        try:
            image = Image.open(path).convert("RGBA")
        except OSError:
            print(f"warning: texture {path} not found; it reads as mid grey")
            image = Image.new("RGBA", (1, 1), (128, 128, 128, 255))
        # Raw texels: the generated code converts from the authored colorspace itself (never an -srgb format).
        texture = device.create_texture(size=(image.width, image.height, 1), format=wgpu.TextureFormat.rgba8unorm,
                                        usage=wgpu.TextureUsage.TEXTURE_BINDING | wgpu.TextureUsage.COPY_DST)
        device.queue.write_texture({"texture": texture}, image.tobytes(),
                                   {"bytes_per_row": 4 * image.width, "rows_per_image": image.height},
                                   (image.width, image.height, 1))
        address_u, address_v = t["address"]
        nearest = t["filter"] == "closest"
        sampler = device.create_sampler(address_mode_u=ADDRESS[address_u], address_mode_v=ADDRESS[address_v],
                                        mag_filter="nearest" if nearest else "linear",
                                        min_filter="nearest" if nearest else "linear")
        entries.append({"binding": t["binding"], "resource": texture.create_view()})
        entries.append({"binding": t["sampler_binding"], "resource": sampler})
    return entries


def shader(compiled):
    """The generated module and the host around it, joined and validated by Shadewire (naga)."""
    reflection = compiled.reflection
    writer = ClosureWriter(reflection["closure"], compiled.model_struct)
    host = "\n".join([host_functions(reflection), LOBE_LIBRARY, writer.write(), ENTRY, VERTEX])
    if writer.unsupported:
        print("lobes this host does not shade:", ", ".join(sorted(writer.unsupported)))
    return wgsl.link(compiled.wgsl, host)


# ---------------------------------------------------------------------------------------------------- render


def render(mtlx_path, material, size):
    text = open(mtlx_path, encoding="utf-8").read()
    ir = mtlx.to_ir(text, path=mtlx_path, material=material, models=["surface"])
    compiled = wgsl.compile(ir, group=MATERIAL_GROUP)
    print(f"{ir.model}: {len(compiled.reflection['closure']['nodes'])} closure nodes, "
          f"{len(compiled.reflection['uniforms'])} uniforms, {len(compiled.reflection['textures'])} textures")

    adapter = wgpu.gpu.request_adapter_sync(power_preference="high-performance")
    device = adapter.request_device_sync()
    module = device.create_shader_module(code=shader(compiled))
    color_format = wgpu.TextureFormat.rgba8unorm_srgb
    pipeline = device.create_render_pipeline(
        layout="auto",
        vertex={
            "module": module,
            "entry_point": "vs_main",
            "buffers": [{
                "array_stride": 11 * 4,
                "attributes": [
                    {"format": wgpu.VertexFormat.float32x3, "offset": 0, "shader_location": 0},
                    {"format": wgpu.VertexFormat.float32x3, "offset": 12, "shader_location": 1},
                    {"format": wgpu.VertexFormat.float32x3, "offset": 24, "shader_location": 2},
                    {"format": wgpu.VertexFormat.float32x2, "offset": 36, "shader_location": 3},
                ],
            }],
        },
        primitive={"topology": wgpu.PrimitiveTopology.triangle_list, "cull_mode": wgpu.CullMode.back},
        depth_stencil={"format": wgpu.TextureFormat.depth24plus, "depth_write_enabled": True,
                       "depth_compare": wgpu.CompareFunction.less},
        multisample={"count": 4},
        fragment={"module": module, "entry_point": "fs_main", "targets": [{"format": color_format}]},
    )

    # Group 0: the scene. `Scene` (in wgsl_lobes.ENTRY) is four vec3 padded to 16 bytes each.
    eye = [3.2, 2.4, 4.0]
    scene = struct.pack("<16f", *eye, 0, *normalize([0.6, 1.0, 0.8]), 0, 3.0, 2.9, 2.7, 0, 0.35, 0.38, 0.45, 0)
    view_proj = multiply(perspective(math.radians(35), 1.0, 0.1, 100.0), look_at(eye, [0, 0, 0], [0, 1, 0]))
    scene_buffer = device.create_buffer_with_data(data=scene, usage=wgpu.BufferUsage.UNIFORM)
    camera_buffer = device.create_buffer_with_data(data=struct.pack("<16f", *view_proj), usage=wgpu.BufferUsage.UNIFORM)
    scene_group = device.create_bind_group(layout=pipeline.get_bind_group_layout(0), entries=[
        {"binding": 0, "resource": {"buffer": scene_buffer, "size": len(scene)}},
        {"binding": 1, "resource": {"buffer": camera_buffer, "size": 64}},
    ])
    groups = [scene_group]
    entries = material_entries(device, compiled.reflection, os.path.dirname(mtlx_path))
    if entries:
        groups.append(device.create_bind_group(layout=pipeline.get_bind_group_layout(MATERIAL_GROUP), entries=entries))

    vertices, indices, index_count = cube()
    vertex_buffer = device.create_buffer_with_data(data=vertices, usage=wgpu.BufferUsage.VERTEX)
    index_buffer = device.create_buffer_with_data(data=indices, usage=wgpu.BufferUsage.INDEX)

    def target(fmt, samples, usage):
        return device.create_texture(size=(size, size, 1), format=fmt, sample_count=samples, usage=usage)

    msaa = target(color_format, 4, wgpu.TextureUsage.RENDER_ATTACHMENT)
    resolved = target(color_format, 1, wgpu.TextureUsage.RENDER_ATTACHMENT | wgpu.TextureUsage.COPY_SRC)
    depth = target(wgpu.TextureFormat.depth24plus, 4, wgpu.TextureUsage.RENDER_ATTACHMENT)

    encoder = device.create_command_encoder()
    pass_ = encoder.begin_render_pass(
        color_attachments=[{"view": msaa.create_view(), "resolve_target": resolved.create_view(),
                            "clear_value": (0.08, 0.08, 0.09, 1.0), "load_op": "clear", "store_op": "discard"}],
        depth_stencil_attachment={"view": depth.create_view(), "depth_clear_value": 1.0,
                                  "depth_load_op": "clear", "depth_store_op": "discard"},
    )
    pass_.set_pipeline(pipeline)
    for i, group in enumerate(groups):
        pass_.set_bind_group(i, group)
    pass_.set_vertex_buffer(0, vertex_buffer)
    pass_.set_index_buffer(index_buffer, wgpu.IndexFormat.uint32)
    pass_.draw_indexed(index_count)
    pass_.end()
    device.queue.submit([encoder.finish()])

    pixels = device.queue.read_texture({"texture": resolved}, {"bytes_per_row": 4 * size, "rows_per_image": size},
                                       (size, size, 1))
    return Image.frombytes("RGBA", (size, size), bytes(pixels))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("mtlx")
    parser.add_argument("--material")
    parser.add_argument("--out", default="cube.png")
    parser.add_argument("--size", type=int, default=800)
    args = parser.parse_args()
    try:
        image = render(args.mtlx, args.material, args.size)
    except shadewire.ShadewireError as e:
        raise SystemExit(f"shadewire: {e}")
    image.save(args.out)
    print("wrote", args.out)


if __name__ == "__main__":
    main()
