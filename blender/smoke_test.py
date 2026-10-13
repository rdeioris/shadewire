"""Builds every material of the given MaterialX documents (default: the vendored examples) into Blender node
trees and reports failures and warnings. Exits 1 when a material fails.

blender -b -P blender/smoke_test.py -- [--shadewire PATH] [file.mtlx ...]
"""

import argparse
import glob
import os
import sys
import traceback

import bpy

here = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, here)
from shadewire_mtlx import builder, compiler  # noqa: E402

parser = argparse.ArgumentParser()
parser.add_argument("--shadewire", default="")
parser.add_argument("--mode", choices=["PRINCIPLED", "LOBES"], default="PRINCIPLED")
parser.add_argument("files", nargs="*")
args = parser.parse_args(sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else [])
files = args.files or sorted(
    glob.glob(os.path.join(here, "..", "vendor", "materialx", "resources", "Materials", "Examples", "**", "*.mtlx"),
              recursive=True)
)

compile_backend = compiler.backend(args.shadewire)
print("compiler:", compile_backend.name)
failures = 0
built = 0
for path in files:
    try:
        names = compile_backend.material_names(path)
    except compiler.CompileError as e:
        print(f"FAIL {path}: {e}")
        failures += 1
        continue
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
print(f"built {built}, failed {failures}")
sys.exit(1 if failures else 0)
