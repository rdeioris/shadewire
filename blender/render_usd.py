"""Renders a USD scene with Cycles, its MaterialX materials built by the Shadewire add-on.

blender -b -P blender/render_usd.py -- <scene.usd> <materials.mtlx> <out.png> [--samples N] [--size W H]
    [--shadewire PATH] [--denoise] [--editable] [--exposure STOPS] [--view-transform NAME]

Blender's USD importer makes an empty material for each binding into the `.mtlx` reference; they are rebuilt
from the document by name, as the add-on's "Fill Materials from MaterialX" does.
"""

import argparse
import os
import sys

import bpy

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from shadewire_mtlx import builder, compiler  # noqa: E402

parser = argparse.ArgumentParser()
parser.add_argument("scene")
parser.add_argument("mtlx")
parser.add_argument("out")
parser.add_argument("--samples", type=int, default=1024)
parser.add_argument("--size", type=int, nargs=2, default=None)
parser.add_argument("--shadewire", default="")
parser.add_argument("--denoise", action="store_true")
parser.add_argument("--editable", action="store_true")
parser.add_argument("--exposure", type=float, default=0.0)
parser.add_argument("--view-transform", default="AgX")
args = parser.parse_args(sys.argv[sys.argv.index("--") + 1 :])

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.wm.usd_import(filepath=args.scene)

exe = compiler.executable(args.shadewire)
names = compiler.material_names(exe, args.mtlx)
for name in names:
    material = bpy.data.materials.get(name)
    if material is None:
        continue
    doc = compiler.compile_material(exe, args.mtlx, name, args.editable)
    warnings = builder.build_material(material, doc, os.path.dirname(args.mtlx))
    print(f"material {name}: {len(material.node_tree.nodes)} nodes", *(f"\n  warning: {w}" for w in warnings))

scene = bpy.context.scene
scene.camera = next(o for o in bpy.data.objects if o.type == "CAMERA")
scene.render.engine = "CYCLES"
prefs = bpy.context.preferences.addons["cycles"].preferences
for device_type in ("OPTIX", "CUDA", "HIP", "METAL", "ONEAPI"):
    try:
        prefs.compute_device_type = device_type
    except TypeError:
        continue
    prefs.get_devices()
    gpus = [d for d in prefs.devices if d.type == device_type]
    if gpus:
        for d in prefs.devices:
            d.use = d.type == device_type
        scene.cycles.device = "GPU"
        break
scene.cycles.samples = args.samples
scene.cycles.use_denoising = args.denoise
if args.size:
    scene.render.resolution_x, scene.render.resolution_y = args.size
scene.render.resolution_percentage = 100
world = bpy.data.worlds.new("black")
world.use_nodes = True
world.node_tree.nodes["Background"].inputs["Strength"].default_value = 0.0
scene.world = world
scene.view_settings.view_transform = args.view_transform
scene.view_settings.exposure = args.exposure
scene.render.filepath = args.out
print("device:", scene.cycles.device, [d.name for d in prefs.devices if d.use],
      "clamp indirect", scene.cycles.sample_clamp_indirect, "filter glossy", scene.cycles.blur_glossy)
bpy.ops.render.render(write_still=True)
