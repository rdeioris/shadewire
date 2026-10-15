"""Builds every material of the given MaterialX documents (default: the vendored examples) into Blender node
trees and reports failures and warnings. Each built material's graph must survive dump → load → dump, and its
export back to MaterialX through Shadewire must compile again. USD scenes among the files are imported with the
add-on's USD hook registered, and every material with a MaterialX surface must arrive built (and pass the same two
checks) and every texture load; with --flatten each scene is also imported flattened (its MaterialX inline, which the hook writes back as a
document). Exits 1 when a material fails.

blender -b -P blender/smoke_test.py -- [--shadewire PATH] [--mode PRINCIPLED|LOBES] [--flatten] [--installed]
    [file.mtlx|scene.usd ...]

--installed tests the extension Blender has installed and enabled (from a zip, with its bundled module) instead of
the add-on in this folder.
"""

import argparse
import collections
import difflib
import glob
import importlib.util
import os
import re
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
builder, compiler, graph, usd = addon.builder, addon.compiler, addon.graph, addon.usd
compile_backend = compiler.backend(args.shadewire)
print("add-on:", addon.__name__, os.path.dirname(addon.__file__))
print("compiler:", compile_backend.name)
usd.defaults.update(usd_mode=args.mode, executable=args.shadewire)


def check_mtlx(path):
    """Counts: built, round-tripped, re-exported, failed."""
    counts = collections.Counter()
    try:
        names = compile_backend.material_names(path)
    except compiler.CompileError as e:
        print(f"FAIL {path}: {e}")
        counts["failed"] += 1
        return counts
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
        except Exception:
            counts["failed"] += 1
            print(f"FAIL {label}:\n{traceback.format_exc()}")
            continue
        counts["built"] += 1
        print(f"ok   {label}: {len(material.node_tree.nodes)} nodes", *(f"\n     warning: {w}" for w in warnings))
        counts["round-tripped" if round_trips(material, label) else "failed"] += 1
        counts["re-exported" if re_exports(material, label) else "failed"] += 1
    return counts


def comparable(doc):
    """A graph document without what a load may change: the producer and the material's name (a loaded material
    is a new one, numbered when the original is still there)."""
    doc = dict(doc, materials=[dict(entry, name="") for entry in doc["materials"]])
    doc.pop("producer")
    return doc


def round_trips(material, label):
    """Whether the material's graph survives dump → text → load → dump with no load warning; the loaded material is
    removed afterwards."""
    try:
        original = graph.dump([material])
        made, warnings = graph.load(graph.from_text(graph.to_text(original)))
    except Exception:
        print(f"FAIL {label}: graph round trip:\n{traceback.format_exc()}")
        return False
    again = graph.dump(made)
    for loaded in made:
        bpy.data.materials.remove(loaded)
    if warnings:
        print(f"FAIL {label}: graph load warnings:", *(f"\n     {w}" for w in warnings))
        return False
    if comparable(again) != comparable(original):
        diff = difflib.unified_diff(graph.to_text(comparable(original)).splitlines(),
                                    graph.to_text(comparable(again)).splitlines(), lineterm="")
        print(f"FAIL {label}: dump → load → dump differs:\n" + "\n".join(list(diff)[:40]))
        return False
    return True


def material_element(name):
    """The `surfacematerial` name the MaterialX writer gives a material: characters outside `[A-Za-z0-9_]` become
    `_`, a leading digit is prefixed with `_`."""
    name = re.sub(r"[^A-Za-z0-9_]", "_", name) or "node"
    return "_" + name if name[0].isdigit() else name


def re_exports(material, label):
    """Whether the material, exported to MaterialX through Shadewire, compiles again; export warnings are printed."""
    try:
        text, warnings = compile_backend.to_mtlx(graph.to_text(graph.dump([material])), material.name, material.name)
        for warning in warnings:
            print(f"     export warning: {warning}")
        compile_backend.compile_text(text, None, material_element(material.name), False, args.mode)
    except (compiler.CompileError, ValueError) as e:
        print(f"FAIL {label}: re-export: {e}")
        return False
    return True


def check_usd(path):
    """Counts: built, round-tripped, re-exported, failed; the scene's MaterialX materials as Blender's USD importer
    leaves them, each built one also checked as `check_mtlx` checks a material."""
    stage = Usd.Stage.Open(path)
    expected = sorted(p.GetName() for p in stage.Traverse()
                      if p.IsA(UsdShade.Material) and UsdShade.Material(p).GetSurfaceOutput("mtlx"))
    bpy.ops.wm.read_homefile(use_empty=True)
    try:
        bpy.ops.wm.usd_import(filepath=path)
    except RuntimeError as e:
        print(f"FAIL {os.path.basename(path)}: {e}")
        return collections.Counter(failed=1)
    counts = collections.Counter()
    for name in expected:
        label = f"{os.path.basename(path)}:{name}"
        material = bpy.data.materials.get(name)
        # Built by the hook (which marks it), not by Blender's fallback after a failure.
        if material and "shadewire_mode" in material and any(
                n.bl_idname == "ShaderNodeOutputMaterial" and n.inputs["Surface"].is_linked
                for n in material.node_tree.nodes):
            counts["built"] += 1
            print(f"ok   {label}: {len(material.node_tree.nodes)} nodes")
            counts["round-tripped" if round_trips(material, label) else "failed"] += 1
            counts["re-exported" if re_exports(material, label) else "failed"] += 1
        else:
            counts["failed"] += 1
            print(f"FAIL {label}: not built by the hook")
    for image in bpy.data.images:
        if image.size[0] == 0:
            counts["failed"] += 1
            print(f"FAIL {os.path.basename(path)}: texture {image.filepath} did not load")
    return counts


counts = collections.Counter()
with tempfile.TemporaryDirectory() as folder:
    for path in files:
        if not path.lower().endswith(USD):
            counts += check_mtlx(path)
        else:
            counts += check_usd(path)
            if args.flatten:
                flat = os.path.join(folder, os.path.splitext(os.path.basename(path))[0] + ".flat.usda")
                Usd.Stage.Open(path).Flatten().Export(flat)
                counts += check_usd(flat)
print(f"built {counts['built']}, round-tripped {counts['round-tripped']}, re-exported {counts['re-exported']}, "
      f"failed {counts['failed']}")
sys.exit(1 if counts["failed"] else 0)
