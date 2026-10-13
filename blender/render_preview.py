"""Renders the extensions.blender.org preview image: ten vendored MaterialX examples on spheres, each imported with the
installed extension's File > Import operator, rendered by Cycles under one of Blender's bundled studio lights (`datafiles/studiolights/world`).

blender -b -P blender/render_preview.py -- out.png [--samples N] [--mode PRINCIPLED|LOBES] [--size W H]
    [--light STUDIO] [--strength S]

Run it with the extension installed and enabled (see blender/README.md: a throwaway profile and
`extension install-file -e`), so the image shows what users get.
"""

import argparse
import os
import sys

import bpy

EXAMPLES = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "vendor", "materialx", "resources",
                        "Materials", "Examples")
# (document, label), top row then bottom row.
MATERIALS = [
    ("StandardSurface/standard_surface_gold.mtlx", "Gold"),
    ("StandardSurface/standard_surface_jade.mtlx", "Jade"),
    ("OpenPbr/open_pbr_honey.mtlx", "Honey"),
    ("StandardSurface/standard_surface_carpaint.mtlx", "Car paint"),
    ("StandardSurface/standard_surface_copper.mtlx", "Copper"),
    ("StandardSurface/standard_surface_marble_solid.mtlx", "Marble"),
    ("OpenPbr/open_pbr_pearl.mtlx", "Pearl"),
    ("StandardSurface/standard_surface_glass_tinted.mtlx", "Tinted glass"),
    ("OpenPbr/open_pbr_velvet.mtlx", "Velvet"),
    ("StandardSurface/standard_surface_thin_film.mtlx", "Thin film"),
]

parser = argparse.ArgumentParser()
parser.add_argument("out")
parser.add_argument("--samples", type=int, default=256)
parser.add_argument("--mode", choices=["PRINCIPLED", "LOBES"], default="PRINCIPLED")
parser.add_argument("--size", type=int, nargs=2, default=[1920, 1080])
parser.add_argument("--light", default="interior", help="a studio light: city, courtyard, forest, interior, ...")
parser.add_argument("--strength", type=float, default=1.1)
args = parser.parse_args(sys.argv[sys.argv.index("--") + 1 :])

if not hasattr(bpy.ops.shadewire, "import_mtlx"):
    sys.exit("the shadewire extension is not installed and enabled")
bpy.ops.wm.read_homefile(use_empty=True)
scene = bpy.context.scene


def emission(name, color, strength):
    material = bpy.data.materials.new(name)
    material.use_nodes = True
    nodes = material.node_tree.nodes
    nodes.clear()
    out = nodes.new("ShaderNodeOutputMaterial")
    shader = nodes.new("ShaderNodeEmission")
    shader.inputs["Color"].default_value = (*color, 1.0)
    shader.inputs["Strength"].default_value = strength
    material.node_tree.links.new(shader.outputs[0], out.inputs[0])
    return material


label_material = emission("label", (1.0, 1.0, 1.0), 0.75)
for i, (document, label) in enumerate(MATERIALS):
    before = set(bpy.data.materials)
    bpy.ops.shadewire.import_mtlx(filepath=os.path.join(EXAMPLES, document), mode=args.mode)
    made = [m for m in bpy.data.materials if m not in before]
    if not made:
        sys.exit(f"{document}: no material imported")
    row, column = divmod(i, 5)
    x, z = (column - 2) * 2.1, 0.95 - row * 2.35
    bpy.ops.mesh.primitive_uv_sphere_add(segments=96, ring_count=48, radius=0.85, location=(x, 0, z))
    sphere = bpy.context.object
    bpy.ops.object.shade_smooth()
    sphere.data.materials.append(made[0])
    bpy.ops.object.text_add(location=(x, 0, z - 1.17), rotation=(1.5708, 0, 0))
    text = bpy.context.object
    text.data.body = label
    text.data.align_x = "CENTER"
    text.data.size = 0.2
    text.data.materials.append(label_material)

# Lit by the studio light; seen by the camera as a dark backdrop.
world = bpy.data.worlds.new("preview")
scene.world = world
world.use_nodes = True
nodes, links = world.node_tree.nodes, world.node_tree.links
nodes.clear()
environment = nodes.new("ShaderNodeTexEnvironment")
studio = os.path.join(bpy.utils.system_resource("DATAFILES"), "studiolights", "world", args.light + ".exr")
environment.image = bpy.data.images.load(studio)
light = nodes.new("ShaderNodeBackground")
light.inputs["Strength"].default_value = args.strength
backdrop = nodes.new("ShaderNodeBackground")
backdrop.inputs["Color"].default_value = (0.012, 0.013, 0.016, 1.0)
path = nodes.new("ShaderNodeLightPath")
mix = nodes.new("ShaderNodeMixShader")
out = nodes.new("ShaderNodeOutputWorld")
links.new(environment.outputs["Color"], light.inputs["Color"])
links.new(path.outputs["Is Camera Ray"], mix.inputs[0])
links.new(light.outputs[0], mix.inputs[1])
links.new(backdrop.outputs[0], mix.inputs[2])
links.new(mix.outputs[0], out.inputs[0])

bpy.ops.object.camera_add(location=(0, -15.5, -0.2), rotation=(1.5708, 0, 0))
scene.camera = bpy.context.object
scene.camera.data.lens = 50

scene.render.engine = "CYCLES"
cycles = scene.cycles
cycles.samples = args.samples
cycles.use_denoising = True
preferences = bpy.context.preferences.addons["cycles"].preferences
for backend in ("OPTIX", "CUDA", "HIP", "METAL", "ONEAPI"):
    try:
        preferences.compute_device_type = backend
    except TypeError:
        continue
    preferences.get_devices()
    if any(d.type == backend for d in preferences.devices):
        for device in preferences.devices:
            device.use = device.type == backend
        cycles.device = "GPU"
        break
print("cycles device:", cycles.device, preferences.compute_device_type)
scene.render.resolution_x, scene.render.resolution_y = args.size
scene.render.resolution_percentage = 100
scene.view_settings.view_transform = "AgX"
scene.render.image_settings.file_format = "PNG"
scene.render.filepath = os.path.abspath(args.out)
bpy.ops.render.render(write_still=True)
print("wrote", scene.render.filepath)
