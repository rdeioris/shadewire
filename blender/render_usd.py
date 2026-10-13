"""Renders a USD scene with Cycles or EEVEE, its MaterialX materials built by the add-on's USD hook as Blender's
USD importer imports them.

blender -b -P blender/render_usd.py -- <scene.usd> <out.png> [--engine CYCLES|EEVEE] [--samples N] [--size W H]
    [--shadewire EXECUTABLE] [--mode PRINCIPLED|LOBES] [--denoise] [--editable] [--exposure STOPS]
    [--view-transform NAME] [--world STRENGTH]
"""

import argparse
import os
import sys

import bpy

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import shadewire_mtlx  # noqa: E402
from shadewire_mtlx import usd  # noqa: E402

parser = argparse.ArgumentParser()
parser.add_argument("scene")
parser.add_argument("out")
parser.add_argument("--engine", choices=["CYCLES", "EEVEE"], default="CYCLES")
parser.add_argument("--samples", type=int, default=1024)
parser.add_argument("--size", type=int, nargs=2, default=None)
parser.add_argument("--shadewire", default="")
parser.add_argument("--denoise", action="store_true")
parser.add_argument("--editable", action="store_true")
parser.add_argument("--mode", choices=["PRINCIPLED", "LOBES"], default="PRINCIPLED")
parser.add_argument("--exposure", type=float, default=0.0)
parser.add_argument("--view-transform", default="AgX")
parser.add_argument("--world", type=float, default=0.0, help="the strength of a grey world")
args = parser.parse_args(sys.argv[sys.argv.index("--") + 1 :])

usd.defaults.update(usd_mode=args.mode, usd_editable=args.editable, executable=args.shadewire)
shadewire_mtlx.register()
bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.wm.usd_import(filepath=args.scene)
for material in bpy.data.materials:
    if material.node_tree:
        print(f"material {material.name}: {len(material.node_tree.nodes)} nodes")

scene = bpy.context.scene
scene.camera = next(o for o in bpy.data.objects if o.type == "CAMERA")
if args.engine == "EEVEE":
    scene.render.engine = "BLENDER_EEVEE"
    scene.eevee.taa_render_samples = args.samples
else:
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
    print("device:", scene.cycles.device, [d.name for d in prefs.devices if d.use])
if args.size:
    scene.render.resolution_x, scene.render.resolution_y = args.size
scene.render.resolution_percentage = 100
world = bpy.data.worlds.new("world")
world.use_nodes = True
background = world.node_tree.nodes["Background"]
background.inputs["Color"].default_value = (0.5, 0.5, 0.5, 1.0)
background.inputs["Strength"].default_value = args.world
scene.world = world
scene.view_settings.view_transform = args.view_transform
scene.view_settings.exposure = args.exposure
scene.render.filepath = args.out
bpy.ops.render.render(write_still=True)
