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
from pxr import Usd, UsdShade

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
    """The configured executable, "" for the module when it imports with its Blender front end, else a built
    executable in target/."""
    if configured:
        return configured
    try:
        import shadewire
    except ImportError:
        pass
    else:
        if hasattr(shadewire, "blender"):
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
        check("element named NG_A" in str(e), f"a repeated material name is an error naming the element: {e}")
    else:
        check(False, "a repeated element name raises CompileError")


def diffuse_material(name):
    """A material that is a lone Diffuse BSDF (a closure tree, no Principled BSDF)."""
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nodes = m.node_tree.nodes
    nodes.remove(nodes["Principled BSDF"])
    diffuse = nodes.new("ShaderNodeBsdfDiffuse")
    m.node_tree.links.new(diffuse.outputs["BSDF"], nodes["Material Output"].inputs["Surface"])
    return m


def test_merge_lobe_materials():
    bpy.ops.wm.read_homefile(use_empty=True)
    one, two = diffuse_material("One"), diffuse_material("Two")
    doc = graph.to_text(graph.dump([one, two]))
    merged = compiler.merge_documents([backend.to_mtlx(doc, "One", "One")[0], backend.to_mtlx(doc, "Two", "Two")[0]])
    check(merged.count("<surfacematerial ") == 2, "two closure-tree materials in one document")
    valid, message = parse_mtlx(merged)
    check(valid in (True, None), f"MaterialX parses the merged lobe document: {message}")
    for name in ("One", "Two"):
        doc = backend.compile_text(merged, None, name, False, "LOBES")
        check(doc["format"] == "shadewire-ir", f"{name} compiles from the merged lobe document")


def test_merge_unnamed_elements():
    document = '<?xml version="1.0"?>\n<materialx version="1.39">\n  <variantset />\n</materialx>\n'
    merged = compiler.merge_documents([document, document])
    check(merged.count("<variantset") == 2, f"elements without a name are neither deduplicated nor an error: {merged}")


def test_non_ascii_path():
    bpy.ops.wm.read_homefile(use_empty=True)
    m = textured_material("Accents")
    image = m.node_tree.nodes["Albedo"].image
    name = "tèxture 日本.png"
    image.filepath_raw = os.path.join(textures, name)
    image.save()
    text, _ = backend.to_mtlx(graph.to_text(graph.dump([m])), m.name, "Accents")
    check(name in text, "a non-ASCII image path comes back unchanged")
    # The round trip also passes where the locale is already UTF-8, so the decoding is checked directly.
    if isinstance(backend, compiler.ExecutableBackend):
        calls = []
        run = compiler.subprocess.run
        compiler.subprocess.run = lambda *a, **kw: calls.append(kw) or run(*a, **kw)
        try:
            backend.run_with_stderr(["--help"])
        finally:
            compiler.subprocess.run = run
        check(bool(calls) and calls[0].get("encoding") == "utf-8",
              f"the executable's output is decoded as UTF-8: {calls}")


def fake_executable(stderr):
    """An executable that prints `stderr` and exits 2, as clap does for an argument it does not know."""
    folder = tempfile.mkdtemp(prefix="shadewire-fake-")
    if sys.platform == "win32":
        path = os.path.join(folder, "shadewire.bat")
        script = f"@echo off\r\necho {stderr} 1>&2\r\nexit /b 2\r\n"
    else:
        path = os.path.join(folder, "shadewire")
        script = f"#!/bin/sh\necho \"{stderr}\" >&2\nexit 2\n"
    with open(path, "w", newline="") as f:
        f.write(script)
    os.chmod(path, 0o755)
    return path


def test_old_executable_hint():
    doc = graph.to_text({"format": "shadewire-blender-graph", "version": 1, "materials": [{"name": "M"}]})
    for message in ("error: unrecognized subcommand 'mtlx'", "error: unexpected argument '--material' found",
                    "error: invalid value 'blender' for '--format'"):
        try:
            compiler.ExecutableBackend(fake_executable(message)).to_mtlx(doc, "M", "M")
        except compiler.CompileError as e:
            check("does not read Blender graphs" in str(e), f"{message!r} gets the version hint: {e}")
        else:
            check(False, f"{message!r} raises CompileError")
    try:
        compiler.ExecutableBackend(fake_executable("error: boom")).to_mtlx(doc, "M", "M")
    except compiler.CompileError as e:
        check("does not read Blender graphs" not in str(e), f"another failure gets no hint: {e}")


class StaleModule:
    """A `shadewire` module from before the Blender front end: no `blender` attribute."""

    @staticmethod
    def version():
        return "0.0.9"


def test_module_without_front_end():
    saved = sys.modules.get("shadewire"), compiler.shutil.which
    sys.modules["shadewire"] = StaleModule
    try:
        compiler.shutil.which = lambda name: executable or None
        chosen = compiler.backend("")
        check(isinstance(chosen, compiler.ExecutableBackend), f"a module without a front end is skipped: {chosen.name}")
        compiler.shutil.which = lambda name: None
        try:
            compiler.backend("")
        except compiler.CompileError as e:
            check("module 0.0.9 has no Blender front end" in str(e), f"the error names the module: {e}")
        else:
            check(False, "no executable and a stale module raises CompileError")
        try:
            compiler.ModuleBackend(StaleModule).to_mtlx("{}", "M", "M")
        except compiler.CompileError as e:
            check("has no Blender front end" in str(e), f"to_mtlx guards a stale module: {e}")
        else:
            check(False, "to_mtlx on a stale module raises CompileError")
    finally:
        compiler.shutil.which = saved[1]
        if saved[0] is None:
            del sys.modules["shadewire"]
        else:
            sys.modules["shadewire"] = saved[0]


def read(path):
    with open(path, encoding="utf-8") as f:
        return f.read()


def select_only(obj):
    for o in bpy.context.view_layer.objects:
        o.select_set(o == obj)
    bpy.context.view_layer.objects.active = obj


def cube_with(material):
    bpy.ops.mesh.primitive_cube_add()
    cube = bpy.context.object
    cube.data.materials.append(material)
    return cube


def test_export_operator():
    with tempfile.TemporaryDirectory() as folder:
        bpy.ops.wm.read_homefile(use_empty=True)
        m = textured_material("Wood")
        select_only(cube_with(m))
        path = os.path.join(folder, "wood.mtlx")
        result = bpy.ops.shadewire.export_mtlx(filepath=path, scope="ACTIVE", copy_textures=True,
                                               executable=executable)
        check(result == {"FINISHED"}, f"export finished: {result}")
        text = read(path)
        check('<surfacematerial name="Wood"' in text, "the material is named after Blender's")
        files = [line.strip() for line in text.splitlines() if 'type="filename"' in line]
        check(files and all('value="textures/' in line for line in files),
              f"image paths are relative to the document: {files}")
        written = os.listdir(os.path.join(folder, "textures"))
        check(any(name.startswith("Wood") for name in written), f"the texture was written: {written}")
        valid, message = parse_mtlx(text)
        check(valid in (True, None), f"MaterialX parses the export: {message}")
        # Several materials, absolute paths, inline.
        other = textured_material("Metal")
        other.node_tree.nodes["Principled BSDF"].inputs["Metallic"].default_value = 1.0
        bpy.context.object.data.materials.append(other)
        path = os.path.join(folder, "both.mtlx")
        result = bpy.ops.shadewire.export_mtlx(filepath=path, scope="SELECTED", copy_textures=False, inline=True,
                                               executable=executable)
        check(result == {"FINISHED"}, f"export of two materials finished: {result}")
        text = read(path)
        check(text.count("<surfacematerial ") == 2, "both materials are in the document")
        check('nodedef="ND_convert_' not in text, "inline expands the library nodes")
        check('value="textures/' not in text, "without Copy Textures the paths are not rewritten")
        path = os.path.join(folder, "folded.mtlx")
        result = bpy.ops.shadewire.export_mtlx(filepath=path, scope="SELECTED", copy_textures=False,
                                               executable=executable)
        check(result == {"FINISHED"}, f"export without inline finished: {result}")
        check('nodedef="ND_convert_' in read(path), "without Inline the library nodes stay declared")
        # Nothing to export is an error, nothing written.
        bpy.ops.object.select_all(action="DESELECT")
        path = os.path.join(folder, "none.mtlx")
        try:
            bpy.ops.shadewire.export_mtlx(filepath=path, scope="SELECTED", executable=executable)
        except RuntimeError as e:
            check("no materials" in str(e), f"an empty selection reports: {e}")
        else:
            check(False, "an empty selection is an error")
        check(not os.path.exists(path), "nothing is written on an empty selection")
        # A compiler that does not exist is an error, nothing written.
        select_only(bpy.context.view_layer.objects[0])
        path = os.path.join(folder, "nocompiler.mtlx")
        try:
            bpy.ops.shadewire.export_mtlx(filepath=path, scope="ACTIVE", executable="C:/nope/shadewire.exe")
        except RuntimeError as e:
            check("no Shadewire compiler" in str(e), f"a missing compiler reports: {e}")
        else:
            check(False, "a missing compiler is an error")
        check(not os.path.exists(path), "nothing is written without a compiler")


def test_export_unwritable_image():
    """An image that cannot be copied keeps its path and does not cancel the export."""
    with tempfile.TemporaryDirectory() as folder:
        bpy.ops.wm.read_homefile(use_empty=True)
        m = textured_material("Gone")
        missing = os.path.join(textures, "missing.png").replace("\\", "/")
        image = m.node_tree.nodes["Albedo"].image
        image.source = "FILE"
        image.filepath = missing
        image.reload()
        select_only(cube_with(m))
        path = os.path.join(folder, "gone.mtlx")
        result = bpy.ops.shadewire.export_mtlx(filepath=path, scope="ACTIVE", copy_textures=True,
                                               executable=executable)
        check(result == {"FINISHED"}, f"export with an image that cannot be copied finished: {result}")
        check(missing in read(path), "the image keeps the path it had")


def test_export_menu():
    labels = []

    class Layout:
        def operator(self, idname, text=""):
            labels.append((idname, text))
            return type("Props", (), {})()

    class Menu:
        layout = Layout()

    addon.menu_export(Menu(), bpy.context)
    check(("shadewire.export_mtlx", "MaterialX (Shadewire) (.mtlx)") in labels,
          f"the export menu lists MaterialX: {labels}")


def export_usd(folder, name, **options):
    path = os.path.join(folder, name)
    result = bpy.ops.wm.usd_export(filepath=path, export_materials=True, **options)
    check(result == {"FINISHED"}, f"usd export finished: {result}")
    return path


def material_prim(stage, name="Wood"):
    return stage.GetPrimAtPath(f"/root/_materials/{name}")


def material_spec(path, name="Wood"):
    """The stage of `path` and the material's prim spec in its root layer; the spec reads its layer only while the
    stage is held."""
    stage = Usd.Stage.Open(path)
    return stage, stage.GetRootLayer().GetPrimAtPath(f"/root/_materials/{name}")


def test_usd_export_replaces_blenders_shaders():
    with tempfile.TemporaryDirectory() as folder:
        bpy.ops.wm.read_homefile(use_empty=True)
        m = textured_material("Wood")
        cube_with(m)
        addon.usd.defaults["executable"] = executable
        bpy.context.window_manager.shadewire_usd_materials = True
        path = export_usd(folder, "scene.usda", generate_preview_surface=False)
        mtlx = os.path.join(folder, "scene.mtlx")
        check(os.path.isfile(mtlx), "scene.mtlx is written beside the usd")
        text = read(mtlx)
        check('<surfacematerial name="Wood"' in text, "the material is named after its prim")
        check('value="./textures/' in text, f"textures are the ones Blender exported: {[l for l in text.splitlines() if 'filename' in l]}")
        check(os.path.isdir(os.path.join(folder, "textures")), "Blender wrote the textures folder")
        stage = Usd.Stage.Open(path)
        prim = material_prim(stage)
        check(prim and prim.IsValid(), "the material prim exists")
        children = [c.GetName() for c in prim.GetChildren()]
        check(prim.HasAuthoredReferences(), "the material prim references the mtlx")
        check(UsdShade.Material(prim).GetSurfaceOutput("mtlx").HasConnectedSource() or
              UsdShade.Material(prim).GetSurfaceOutput("mtlx").GetAttr().IsValid(),
              f"the composed material has a MaterialX surface: {children}")
        layer = stage.GetRootLayer()
        spec = layer.GetPrimAtPath("/root/_materials/Wood")
        check("previewShader" not in [c for c in spec.nameChildren.keys()], f"Blender's placeholder shader is gone: {list(spec.nameChildren.keys())}")
        check("outputs:surface" not in spec.properties.keys(), "Blender's surface output is gone")
        # The scene imports back through the hook, with the texture.
        bpy.ops.wm.read_homefile(use_empty=True)
        bpy.ops.wm.usd_import(filepath=path)
        imported = bpy.data.materials.get("Wood")
        check(imported is not None and "shadewire_mode" in imported, "the import hook built the material")
        check(all(image.size[0] > 0 for image in bpy.data.images if image.filepath), "the textures load")


def test_usd_export_keeps_a_requested_preview_surface_and_blenders_output_when_off():
    with tempfile.TemporaryDirectory() as folder:
        bpy.ops.wm.read_homefile(use_empty=True)
        cube_with(textured_material("Wood"))
        addon.usd.defaults["executable"] = executable
        bpy.context.window_manager.shadewire_usd_materials = True
        path = export_usd(folder, "preview.usda", generate_preview_surface=True)
        stage, spec = material_spec(path)
        names = list(spec.nameChildren.keys())
        # Blender names the shaders after their nodes: the image texture is the node named Albedo.
        check(sorted(names) == ["Albedo", "Principled_BSDF", "uvmap"], f"the UsdPreviewSurface network stays: {names}")
        check("outputs:surface" in spec.properties.keys(), "its surface output stays")
        check(spec.hasReferences, "and the mtlx reference is added")
        # Blender's own MaterialX network is replaced by ours.
        path = export_usd(folder, "mx.usda", generate_preview_surface=False, generate_materialx_network=True)
        stage, spec = material_spec(path)
        names = list(spec.nameChildren.keys())
        check(not any(n.startswith("bnode__") or n == "NodeGraphs" for n in names), f"Blender's MaterialX network is gone: {names}")
        check("outputs:mtlx:surface" not in spec.properties.keys() and "config:mtlx:version" not in spec.properties.keys(),
              "its mtlx outputs are gone")
        # The option off: Blender's output untouched, no mtlx.
        bpy.context.window_manager.shadewire_usd_materials = False
        path = export_usd(folder, "off.usda", generate_preview_surface=False)
        stage, spec = material_spec(path)
        check("previewShader" in spec.nameChildren.keys() and not spec.hasReferences, "off: Blender's prims stay")
        check(not os.path.exists(os.path.join(folder, "off.mtlx")), "off: no mtlx")
        bpy.context.window_manager.shadewire_usd_materials = True


def test_usdz_is_left_to_blender():
    with tempfile.TemporaryDirectory() as folder:
        bpy.ops.wm.read_homefile(use_empty=True)
        cube_with(textured_material("Wood"))
        addon.usd.defaults["executable"] = executable
        bpy.context.window_manager.shadewire_usd_materials = True
        path = export_usd(folder, "pack.usdz", generate_preview_surface=False)
        check(os.path.isfile(path), "the usdz is written")
        check(not any(f.endswith(".mtlx") for f in os.listdir(folder)), f"no mtlx beside a usdz: {os.listdir(folder)}")
        stage = Usd.Stage.Open(path)
        prim = material_prim(stage)
        check(prim and "previewShader" in [c.GetName() for c in prim.GetChildren()], "Blender's materials are untouched")


def test_usd_panel():
    panel = bpy.types.SHADEWIRE_PT_usd_export
    check(panel.bl_parent_id == "FILE_PT_operator" and panel.bl_space_type == "FILE_BROWSER", "the panel sits under the operator panel")

    class Op:
        bl_idname = "WM_OT_usd_export"

    class Space:
        active_operator = Op()

    class Ctx:
        space_data = Space()

    check(panel.poll(Ctx()), "the panel polls for the USD export")
    Space.active_operator.bl_idname = "WM_OT_obj_export"
    check(not panel.poll(Ctx()), "and not for other exports")
    check(hasattr(bpy.context.window_manager, "shadewire_usd_materials"), "the window manager has the option")


TESTS = [test_to_mtlx, test_merge_documents, test_merge_lobe_materials, test_merge_unnamed_elements,
         test_non_ascii_path, test_old_executable_hint, test_module_without_front_end,
         test_export_operator, test_export_unwritable_image, test_export_menu,
         test_usd_export_replaces_blenders_shaders,
         test_usd_export_keeps_a_requested_preview_surface_and_blenders_output_when_off, test_usdz_is_left_to_blender,
         test_usd_panel]

if __name__ == "__main__":
    for test in TESTS:
        print("test", test.__name__)
        test()
    print(f"FAILURES {len(failures)}")
    sys.exit(1 if failures else 0)
