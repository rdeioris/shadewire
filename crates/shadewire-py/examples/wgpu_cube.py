"""A cube with a MaterialX material, rendered with wgpu-py and saved as a PNG.

    pip install shadewire wgpu pillow
    python wgpu_cube.py material.mtlx [--material NAME] [--out cube.png] [--size 800]

Shadewire compiles the material (`scene.compile_material`): its pattern graph to `shadewire_eval_surface()`, its
lobes to a closure tree the host shades. This file binds what the reflection asks for (the material's uniform
block, textures and samplers in group 1) next to the scene (group 0), and draws the cube offscreen.
"""

import argparse
import os

import wgpu
from PIL import Image

import scene
import shadewire

ADDRESS = {"periodic": "repeat", "mirror": "mirror-repeat", "clamp": "clamp-to-edge", "constant": "clamp-to-edge"}


def material_entries(device, reflection, base_dir):
    """Bind group entries for the material's uniform block, textures and samplers, as the reflection places them."""
    entries = []
    if reflection["uniform_binding"] is not None:
        data = scene.uniform_bytes(reflection)
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


def render(mtlx_path, material, size):
    compiled, linked = scene.compile_material(mtlx_path, material)
    adapter = wgpu.gpu.request_adapter_sync(power_preference="high-performance")
    device = adapter.request_device_sync()
    module = device.create_shader_module(code=linked)
    color_format = wgpu.TextureFormat.rgba8unorm_srgb
    attribute_format = {2: wgpu.VertexFormat.float32x2, 3: wgpu.VertexFormat.float32x3}
    pipeline = device.create_render_pipeline(
        layout="auto",
        vertex={
            "module": module,
            "entry_point": "vs_main",
            "buffers": [{
                "array_stride": scene.VERTEX_STRIDE,
                "attributes": [{"format": attribute_format[n], "offset": offset, "shader_location": location}
                               for location, n, offset in scene.VERTEX_ATTRIBUTES],
            }],
        },
        primitive={"topology": wgpu.PrimitiveTopology.triangle_list, "cull_mode": wgpu.CullMode.back},
        depth_stencil={"format": wgpu.TextureFormat.depth24plus, "depth_write_enabled": True,
                       "depth_compare": wgpu.CompareFunction.less},
        multisample={"count": 4},
        fragment={"module": module, "entry_point": "fs_main", "targets": [{"format": color_format}]},
    )

    eye = [3.2, 2.4, 4.0]
    scene_data, camera_data = scene.scene_bytes(eye), scene.camera_bytes(eye, 1.0)
    scene_buffer = device.create_buffer_with_data(data=scene_data, usage=wgpu.BufferUsage.UNIFORM)
    camera_buffer = device.create_buffer_with_data(data=camera_data, usage=wgpu.BufferUsage.UNIFORM)
    groups = [device.create_bind_group(layout=pipeline.get_bind_group_layout(0), entries=[
        {"binding": 0, "resource": {"buffer": scene_buffer, "size": len(scene_data)}},
        {"binding": 1, "resource": {"buffer": camera_buffer, "size": len(camera_data)}},
    ])]
    entries = material_entries(device, compiled.reflection, os.path.dirname(mtlx_path))
    if entries:
        layout = pipeline.get_bind_group_layout(scene.MATERIAL_GROUP)
        groups.append(device.create_bind_group(layout=layout, entries=entries))

    vertices, indices, index_count = scene.cube()
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
