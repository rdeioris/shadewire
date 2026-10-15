# SPDX-FileCopyrightText: 2026 Roberto De Ioris
# SPDX-License-Identifier: GPL-3.0-or-later

"""Headless tests of the MaterialX export: compiler access, the export operator, the USD export hooks.

blender -b --python-exit-code 1 -P blender/test_export.py [-- --shadewire PATH]
"""

import argparse
import importlib.util
import os
import sys
import tempfile

import bpy

here = os.path.dirname(os.path.abspath(__file__))
root = os.path.dirname(here)
failures = []
textures = tempfile.mkdtemp(prefix="shadewire-export-")


def check(condition, message):
    if not condition:
        failures.append(message)
        print(f"FAIL {message}")


def load_addon():
    folder = os.path.join(here, "shadewire")
    spec = importlib.util.spec_from_file_location("shadewire_addon", os.path.join(folder, "__init__.py"),
                                                  submodule_search_locations=[folder])
    addon = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = addon
    spec.loader.exec_module(addon)
    addon.register()
    return addon


def find_compiler(configured):
    """The configured executable, "" for the module when it imports, else a built executable in target/."""
    if configured:
        return configured
    try:
        import shadewire  # noqa: F401
    except ImportError:
        pass
    else:
        return ""
    name = "shadewire.exe" if sys.platform == "win32" else "shadewire"
    for profile in ("release", "debug"):
        path = os.path.join(root, "target", profile, name)
        if os.path.isfile(path):
            return path
    sys.exit("no shadewire compiler: `cargo build -p shadewire-cli --release` or pass -- --shadewire PATH")


parser = argparse.ArgumentParser()
parser.add_argument("--shadewire", default="")
args = parser.parse_args(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])
addon = load_addon()
compiler, graph = addon.compiler, addon.graph
executable = find_compiler(args.shadewire)
backend = compiler.backend(executable)
print("compiler:", backend.name)


def textured_material(name="Export Me"):
    """A Principled material with an image on Base Color and a Math node on Roughness."""
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nodes, links = m.node_tree.nodes, m.node_tree.links
    principled = nodes["Principled BSDF"]
    image = bpy.data.images.new(f"{name} albedo", 4, 4)
    image.generated_color = (0.8, 0.2, 0.1, 1.0)
    image.filepath_raw = os.path.join(textures, f"{name} albedo.png")
    image.file_format = "PNG"
    image.save()
    tex = nodes.new("ShaderNodeTexImage")
    tex.name = "Albedo"
    tex.image = image
    links.new(tex.outputs["Color"], principled.inputs["Base Color"])
    math = nodes.new("ShaderNodeMath")
    math.operation = "MULTIPLY"
    math.inputs[0].default_value = 0.5
    math.inputs[1].default_value = 0.8
    links.new(math.outputs["Value"], principled.inputs["Roughness"])
    return m


def parse_mtlx(text):
    """(valid, message) from Blender's bundled MaterialX module, or (None, "no MaterialX module")."""
    try:
        import MaterialX as mx
    except ImportError:
        return None, "no MaterialX module"
    doc = mx.createDocument()
    mx.readFromXmlString(doc, text)
    valid, message = doc.validate()
    return valid, message


def test_to_mtlx():
    bpy.ops.wm.read_homefile(use_empty=True)
    m = textured_material()
    text, warnings = backend.to_mtlx(graph.to_text(graph.dump([m])), m.name, "Export_Me", inline=False)
    check(text.startswith('<?xml version="1.0"?>\n<materialx version="1.39">'), f"to_mtlx header: {text[:80]!r}")
    check('<open_pbr_surface name="Export_Me_shader"' in text, "the Principled BSDF is an open_pbr_surface node")
    check('<surfacematerial name="Export_Me"' in text, "the material is named as asked")
    check("<image " in text and "albedo" in text, "the image is written")
    check("<multiply " in text, "the Math node is a multiply node")
    check(warnings == [], f"no warnings for a plain material: {warnings}")
    valid, message = parse_mtlx(text)
    check(valid in (True, None), f"MaterialX parses the document: {message}")
    inlined, _ = backend.to_mtlx(graph.to_text(graph.dump([m])), m.name, "Export_Me", inline=True)
    check('nodedef="ND_convert_' in text and 'nodedef="ND_convert_' not in inlined and len(inlined) > len(text),
          "inline expands the library nodes")
    # The written text reads again as a MaterialX document, through the same backend.
    doc = backend.compile_text(text, None, "Export_Me", False, "PRINCIPLED")
    check(doc["model"] == "open_pbr_surface", f"the export compiles back: {doc.get('model')}")
    # A lobe material needs the surface model, which the export options accept.
    lobes = bpy.data.materials.new("Lobes")
    lobes.use_nodes = True
    nodes = lobes.node_tree.nodes
    nodes.remove(nodes["Principled BSDF"])
    diffuse = nodes.new("ShaderNodeBsdfDiffuse")
    lobes.node_tree.links.new(diffuse.outputs["BSDF"], nodes["Material Output"].inputs["Surface"])
    text, _ = backend.to_mtlx(graph.to_text(graph.dump([lobes])), lobes.name, "Lobes")
    check("<oren_nayar_diffuse_bsdf" in text and "<surface " in text, "lobes export as a closure tree")
    # An unknown material is an error, not a crash.
    try:
        backend.to_mtlx(graph.to_text(graph.dump([m])), "Nope", "Nope")
    except compiler.CompileError as e:
        check("Nope" in str(e), f"the error names the material: {e}")
    else:
        check(False, "an unknown material raises CompileError")


def test_merge_documents():
    bpy.ops.wm.read_homefile(use_empty=True)
    a, b = textured_material("A"), textured_material("B")
    bump = a.node_tree.nodes.new("ShaderNodeBump")
    a.node_tree.links.new(bump.outputs["Normal"], a.node_tree.nodes["Principled BSDF"].inputs["Normal"])
    doc = graph.to_text(graph.dump([a, b]))
    text_a, _ = backend.to_mtlx(doc, "A", "A")
    text_b, _ = backend.to_mtlx(doc, "B", "B")
    check(compiler.merge_documents([text_a]) == text_a, "one document merges to itself")
    merged = compiler.merge_documents([text_a, text_b])
    check(merged.count("<surfacematerial ") == 2, "two materials in one document")
    check(merged.count("<materialx ") == 1 and merged.count("</materialx>") == 1, "one root")
    valid, message = parse_mtlx(merged)
    check(valid in (True, None), f"MaterialX parses the merged document: {message}")
    # Shadewire's own nodedefs (the bump is built from derivatives) appear once even if both documents embed them.
    text_c, _ = backend.to_mtlx(doc, "A", "C")
    merged = compiler.merge_documents([text_a, text_c])
    names = [line.split('name="')[1].split('"')[0] for line in merged.splitlines() if "<nodedef " in line]
    check(names and len(names) == len(set(names)), f"nodedefs are present once: {names}")
    for name in ("A", "C"):
        check(backend.compile_text(merged, None, name, False, "PRINCIPLED")["model"] == "open_pbr_surface",
              f"{name} compiles from the merged document")
    try:
        compiler.merge_documents([text_a, text_a])
    except compiler.CompileError as e:
        check("A" in str(e), f"a repeated material name is an error naming it: {e}")
    else:
        check(False, "a repeated element name raises CompileError")


TESTS = [test_to_mtlx, test_merge_documents]

if __name__ == "__main__":
    for test in TESTS:
        print("test", test.__name__)
        test()
    print(f"FAILURES {len(failures)}")
    sys.exit(1 if failures else 0)
