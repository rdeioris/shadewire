"""Builds every material of the given MaterialX documents (default: the vendored examples) into Blender node
trees and reports failures and warnings. USD scenes among the files are imported with the add-on's USD hook
registered, and every material with a MaterialX surface must arrive built and every texture load; with --flatten
each scene is also imported flattened (its MaterialX inline, which the hook writes back as a document). Exits 1
when a material fails.

blender -b -P blender/smoke_test.py -- [--shadewire PATH] [--mode PRINCIPLED|LOBES] [--flatten] [--installed]
    [file.mtlx|scene.usd ...]

--installed tests the extension Blender has installed and enabled (from a zip, with its bundled module) instead of
the add-on in this folder.
"""

import argparse
import glob
import importlib.util
import os
import sys
import tempfile
import traceback

import bpy
from pxr import Usd, UsdShade

here = os.path.dirname(os.path.abspath(__file__))


def load_addon(installed):
    """The add-on, registered: as Blender installed and enabled it from an extension zip (`installed`), or from
    `blender/shadewire`, loaded as `shadewire_addon` and with nothing added to sys.path, so that `import shadewire`
    still finds the compiler module rather than the add-on's folder."""
    if installed:
        names = [name for name in sys.modules if name.startswith("bl_ext.") and name.endswith(".shadewire")]
        if not names:
            sys.exit("--installed: no enabled shadewire extension (install the zip with -e first)")
        return sys.modules[names[0]]
    folder = os.path.join(os.path.dirname(os.path.abspath(__file__)), "shadewire")
    spec = importlib.util.spec_from_file_location(
        "shadewire_addon", os.path.join(folder, "__init__.py"), submodule_search_locations=[folder]
    )
    addon = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = addon
    spec.loader.exec_module(addon)
    addon.register()
    return addon


USD = (".usd", ".usda", ".usdc", ".usdz")

parser = argparse.ArgumentParser()
parser.add_argument("--shadewire", default="")
parser.add_argument("--mode", choices=["PRINCIPLED", "LOBES"], default="PRINCIPLED")
parser.add_argument("--flatten", action="store_true")
parser.add_argument("--installed", action="store_true")
parser.add_argument("files", nargs="*")
args = parser.parse_args(sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else [])
files = args.files or sorted(
    glob.glob(os.path.join(here, "..", "vendor", "materialx", "resources", "Materials", "Examples", "**", "*.mtlx"),
              recursive=True)
)

addon = load_addon(args.installed)
builder, compiler, usd = addon.builder, addon.compiler, addon.usd
compile_backend = compiler.backend(args.shadewire)
print("add-on:", addon.__name__, os.path.dirname(addon.__file__))
print("compiler:", compile_backend.name)
usd.defaults.update(usd_mode=args.mode, executable=args.shadewire)


def check_mtlx(path):
    """Failures, built."""
    try:
        names = compile_backend.material_names(path)
    except compiler.CompileError as e:
        print(f"FAIL {path}: {e}")
        return 1, 0
    failures = built = 0
    for name in names:
        label = f"{os.path.basename(path)}:{name}"
        try:
            doc = compile_backend.compile(path, name, False, args.mode)
        except compiler.CompileError as e:
            print(f"SKIP {label}: {str(e).splitlines()[0]}")
            continue
        material = bpy.data.materials.new(name)
        try:
            warnings = builder.build_material(material, doc, os.path.dirname(path))
            built += 1
            print(f"ok   {label}: {len(material.node_tree.nodes)} nodes", *(f"\n     warning: {w}" for w in warnings))
        except Exception:
            failures += 1
            print(f"FAIL {label}:\n{traceback.format_exc()}")
    return failures, built


def check_usd(path):
    """Failures, built: the scene's MaterialX materials as Blender's USD importer leaves them."""
    stage = Usd.Stage.Open(path)
    expected = sorted(p.GetName() for p in stage.Traverse()
                      if p.IsA(UsdShade.Material) and UsdShade.Material(p).GetSurfaceOutput("mtlx"))
    bpy.ops.wm.read_homefile(use_empty=True)
    try:
        bpy.ops.wm.usd_import(filepath=path)
    except RuntimeError as e:
        print(f"FAIL {os.path.basename(path)}: {e}")
        return 1, 0
    failures = built = 0
    for name in expected:
        label = f"{os.path.basename(path)}:{name}"
        material = bpy.data.materials.get(name)
        # Built by the hook (which marks it), not by Blender's fallback after a failure.
        if material and "shadewire_mode" in material and any(
                n.bl_idname == "ShaderNodeOutputMaterial" and n.inputs["Surface"].is_linked
                for n in material.node_tree.nodes):
            built += 1
            print(f"ok   {label}: {len(material.node_tree.nodes)} nodes")
        else:
            failures += 1
            print(f"FAIL {label}: not built by the hook")
    for image in bpy.data.images:
        if image.size[0] == 0:
            failures += 1
            print(f"FAIL {os.path.basename(path)}: texture {image.filepath} did not load")
    return failures, built


failures = built = 0
with tempfile.TemporaryDirectory() as folder:
    for path in files:
        if not path.lower().endswith(USD):
            f, b = check_mtlx(path)
        else:
            f, b = check_usd(path)
            if args.flatten:
                flat = os.path.join(folder, os.path.splitext(os.path.basename(path))[0] + ".flat.usda")
                Usd.Stage.Open(path).Flatten().Export(flat)
                f2, b2 = check_usd(flat)
                f, b = f + f2, b + b2
        failures += f
        built += b
print(f"built {built}, failed {failures}")
sys.exit(1 if failures else 0)
