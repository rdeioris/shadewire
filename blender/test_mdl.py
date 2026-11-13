# SPDX-FileCopyrightText: 2026 Roberto De Ioris
# SPDX-License-Identifier: GPL-3.0-or-later

"""Headless tests of MDL in the add-on: File > Import > MDL in both modes, its search paths, Rebuild and Fill from
a module, and File > Export > MDL for one material and for several.

blender -b --python-exit-code 1 -P blender/test_mdl.py [-- --shadewire PATH]
"""

import argparse
import importlib.util
import os
import shutil
import sys
import tempfile

import bpy

here = os.path.dirname(os.path.abspath(__file__))
root = os.path.dirname(here)
fixtures = os.path.join(root, "crates", "shadewire-mdl", "tests", "fixtures")
failures = []
scratch = tempfile.mkdtemp(prefix="shadewire-mdl-")


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
    """The configured executable, "" for the module when it imports with its MDL front end, else a built executable
    in target/ (or $CARGO_TARGET_DIR)."""
    if configured:
        return configured
    try:
        import shadewire
    except ImportError:
        pass
    else:
        if hasattr(shadewire, "blender") and hasattr(shadewire, "mdl"):
            return ""
    name = "shadewire.exe" if sys.platform == "win32" else "shadewire"
    target = os.environ.get("CARGO_TARGET_DIR") or os.path.join(root, "target")
    for profile in ("release", "debug"):
        path = os.path.join(target, profile, name)
        if os.path.isfile(path):
            return path
    sys.exit("no shadewire compiler: `cargo build -p shadewire-cli --release` or pass -- --shadewire PATH")


parser = argparse.ArgumentParser()
parser.add_argument("--shadewire", default="")
args = parser.parse_args(sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else [])
executable = find_compiler(args.shadewire)
addon = load_addon()
compiler = addon.compiler
print("compiler:", compiler.backend(executable).name)


def imported(path, **options):
    """The materials File > Import > MDL makes from `path`, in order."""
    before = set(bpy.data.materials)
    result = bpy.ops.shadewire.import_mdl(filepath=path, executable=executable, **options)
    check(result == {"FINISHED"}, f"import {os.path.basename(path)} {options}: {result}")
    return [m for m in bpy.data.materials if m not in before]


def kinds(material):
    return sorted(n.bl_idname for n in material.node_tree.nodes)


def test_import_principled():
    made = imported(os.path.join(fixtures, "diffuse.mdl"))
    check(sorted(m.name for m in made) == ["dusty", "reflection", "transmission"], f"diffuse.mdl names: {made}")
    for material in made:
        principled = kinds(material).count("ShaderNodeBsdfPrincipled")
        check(principled == 1, f"{material.name}: {principled} Principled BSDFs, distilled to one expected")
        check(material.get("shadewire_mode") == "PRINCIPLED", f"{material.name}: mode not recorded")
        check(material.get("shadewire_source", "").endswith("diffuse.mdl"), f"{material.name}: source not recorded")


def test_import_lobes():
    (velvet,) = imported(os.path.join(fixtures, "sheen.mdl"), mode="LOBES")
    names = kinds(velvet)
    check("ShaderNodeBsdfPrincipled" not in names, f"velvet in Lobes: {names}")
    check("ShaderNodeBsdfSheen" in names, f"velvet in Lobes has no Sheen BSDF: {names}")
    check(velvet.get("shadewire_mode") == "LOBES", "velvet: mode not recorded")
    # weighted.mdl's layers over nothing and conditional coats lower to empty closures.
    path = os.path.join(fixtures, "weighted.mdl")
    names = compiler.backend(executable).material_names(path)
    made = imported(path, mode="LOBES")
    check(len(made) == len(names), f"weighted.mdl in Lobes: {len(made)} of {len(names)} built")


def test_search_paths():
    library = os.path.join(scratch, "library", "lib")
    os.makedirs(library)
    with open(os.path.join(library, "tints.mdl"), "w", encoding="utf-8") as f:
        f.write("mdl 1.7;\nexport color red() = color(1.0, 0.0, 0.0);\n")
    module = os.path.join(scratch, "uses_tints.mdl")
    with open(module, "w", encoding="utf-8") as f:
        f.write("mdl 1.7;\nimport ::df::*;\nimport ::lib::tints::*;\n"
                "export material tinted() = material(surface: material_surface(scattering: "
                "df::diffuse_reflection_bsdf(tint: lib::tints::red())));\n")
    before = set(bpy.data.materials)
    try:
        bpy.ops.shadewire.import_mdl(filepath=module, executable=executable)
        refused = False
    except RuntimeError as e:  # an operator that reports an error raises it
        refused = "::lib::tints" in str(e)
    check(refused and set(bpy.data.materials) == before, "import without its search path was not refused")
    (tinted,) = imported(module, mdl_paths=os.path.join(scratch, "library"))
    check(tinted.get("shadewire_mdl_paths") == os.path.join(scratch, "library"), "search path not recorded")
    return tinted


def test_rebuild(tinted):
    tinted.node_tree.nodes.clear()
    with bpy.context.temp_override(material=tinted):
        result = bpy.ops.shadewire.rebuild_material(executable=executable)
    check(result == {"FINISHED"}, f"rebuild from MDL: {result}")
    check("ShaderNodeBsdfPrincipled" in kinds(tinted), f"rebuilt tinted: {kinds(tinted)}")


def test_fill():
    existing = bpy.data.materials["velvet"]  # made by test_import_lobes
    existing.node_tree.nodes.clear()
    result = bpy.ops.shadewire.fill_materials(filepath=os.path.join(fixtures, "sheen.mdl"), mode="LOBES",
                                              executable=executable)
    check(result == {"FINISHED"}, f"fill from MDL: {result}")
    check("ShaderNodeBsdfSheen" in kinds(existing), f"filled velvet: {kinds(existing)}")


def principled_material(name, color):
    material = bpy.data.materials.new(name)
    material.use_nodes = True
    bsdf = material.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*color, 1.0)
    bsdf.inputs["Roughness"].default_value = 0.35
    return material


def assigned(materials):
    """A mesh carrying `materials`, the only selected and active object."""
    bpy.ops.object.select_all(action="DESELECT")
    mesh = bpy.data.meshes.new("holder")
    obj = bpy.data.objects.new("holder", mesh)
    bpy.context.scene.collection.objects.link(obj)
    for material in materials:
        mesh.materials.append(material)
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    return obj


def test_export_one():
    red = principled_material("Red Paint", (0.8, 0.05, 0.05))
    assigned([red])
    path = os.path.join(scratch, "one", "paint.mdl")
    os.makedirs(os.path.dirname(path))
    result = bpy.ops.shadewire.export_mdl(filepath=path, scope="SELECTED", executable=executable)
    check(result == {"FINISHED"}, f"export one: {result}")
    check(os.listdir(os.path.dirname(path)) == ["paint.mdl"], f"export one wrote {os.listdir(os.path.dirname(path))}")
    with open(path, encoding="utf-8") as f:
        text = f.read()
    check(text.startswith("mdl 1.7;") and "export material Red_Paint(" in text, "exported module text")
    backend = compiler.backend(executable)
    check(backend.material_names(path) == ["Red_Paint"], "exported module's materials")
    (back,) = imported(path)
    check("ShaderNodeBsdfPrincipled" in kinds(back), f"exported module imports back: {kinds(back)}")


def test_export_several():
    a = principled_material("Blue Paint", (0.1, 0.4, 0.8))
    b = principled_material("Blue-Paint", (0.1, 0.8, 0.2))
    image = bpy.data.images.new("checker", 8, 8)
    image.generated_type = "COLOR_GRID"
    texture = b.node_tree.nodes.new("ShaderNodeTexImage")
    texture.image = image
    b.node_tree.links.new(texture.outputs["Color"], b.node_tree.nodes["Principled BSDF"].inputs["Base Color"])
    assigned([a, b])
    folder = os.path.join(scratch, "several")
    os.makedirs(folder)
    result = bpy.ops.shadewire.export_mdl(filepath=os.path.join(folder, "ignored.mdl"), scope="SELECTED",
                                          executable=executable)
    check(result == {"FINISHED"}, f"export several: {result}")
    written = sorted(os.listdir(folder))
    check(written == ["Blue_Paint.mdl", "Blue_Paint_2.mdl", "textures"], f"export several wrote {written}")
    backend = compiler.backend(executable)
    for name in ("Blue_Paint.mdl", "Blue_Paint_2.mdl"):
        path = os.path.join(folder, name)
        if os.path.isfile(path):
            check(len(backend.material_names(path)) == 1, f"{name}: one material")
            backend.compile(path, None, False, "LOBES")
    if os.path.isfile(os.path.join(folder, "Blue_Paint_2.mdl")):
        with open(os.path.join(folder, "Blue_Paint_2.mdl"), encoding="utf-8") as f:
            check('"./textures/checker.png"' in f.read(), "the textured module references ./textures/checker.png")


USD_MODULE = """mdl 1.7;
import ::df::*;
import ::tex::*;
import ::state::*;
export material plain(color tint = color(0.8)) = material(surface: material_surface(scattering:
    df::diffuse_reflection_bsdf(tint: tint)));
export material textured(uniform texture_2d image = texture_2d()) = material(surface: material_surface(scattering:
    df::diffuse_reflection_bsdf(tint: tex::lookup_color(image,
        float2(state::texture_coordinate(0).x, state::texture_coordinate(0).y)))));
"""

USD_SCENE = """#usda 1.0
def Scope "Looks"
{
    def Material "Plain"
    {
        token outputs:mdl:surface.connect = </Looks/Plain/Shader.outputs:out>
        def Shader "Shader"
        {
            uniform token info:implementationSource = "sourceAsset"
            uniform asset info:mdl:sourceAsset = @./scene_module.mdl@
            uniform token info:mdl:sourceAsset:subIdentifier = "plain"
            color3f inputs:tint = (0.1, 0.9, 0.2)
            token outputs:out
        }
    }
    def Material "Textured"
    {
        token outputs:mdl:surface.connect = </Looks/Textured/Shader.outputs:out>
        def Shader "Shader"
        {
            uniform token info:implementationSource = "sourceAsset"
            uniform asset info:mdl:sourceAsset = @./scene_module.mdl@
            uniform token info:mdl:sourceAsset:subIdentifier = "textured"
            asset inputs:image = @./checker.png@ (
                colorSpace = "sRGB"
            )
            token outputs:out
        }
    }
    def Material "Searched"
    {
        token outputs:mdl:surface.connect = </Looks/Searched/Shader.outputs:out>
        def Shader "Shader"
        {
            uniform token info:implementationSource = "sourceAsset"
            uniform asset info:mdl:sourceAsset = @Paints.mdl@
            uniform token info:mdl:sourceAsset:subIdentifier = "red"
            token outputs:out
        }
    }
}
"""


def test_usd_import():
    """Blender's USD importer hands the add-on the MDL materials of a scene: arguments from the shader's inputs, a
    texture input through the importer, a module found on the MDL search paths."""
    folder = os.path.join(scratch, "usd")
    library = os.path.join(scratch, "usd_library")
    os.makedirs(folder)
    os.makedirs(library)
    with open(os.path.join(folder, "scene_module.mdl"), "w", encoding="utf-8") as f:
        f.write(USD_MODULE)
    with open(os.path.join(library, "Paints.mdl"), "w", encoding="utf-8") as f:
        f.write("mdl 1.7;\nimport ::df::*;\nexport material red() = material(surface: material_surface(scattering: "
                "df::diffuse_reflection_bsdf(tint: color(0.9, 0.1, 0.1))));\n")
    image = bpy.data.images.new("checker", 4, 4)
    image.generated_type = "COLOR_GRID"
    image.filepath_raw = os.path.join(folder, "checker.png")
    image.file_format = "PNG"
    image.save()
    bpy.data.images.remove(image)
    scene = os.path.join(folder, "scene.usda")
    with open(scene, "w", encoding="utf-8") as f:
        f.write(USD_SCENE)
    addon.usd.defaults.update(usd_import=True, usd_mode="PRINCIPLED", usd_editable=False, executable=executable,
                              mdl_paths=library)
    before = set(bpy.data.materials)
    bpy.ops.wm.usd_import(filepath=scene, import_all_materials=True)
    made = {m.name: m for m in bpy.data.materials if m not in before}
    check(sorted(made) == ["Plain", "Searched", "Textured"], f"USD materials: {sorted(made)}")
    for name, material in made.items():
        check("ShaderNodeBsdfPrincipled" in kinds(material), f"USD {name} not built by the add-on: {kinds(material)}")
    if "Plain" in made:
        bsdf = next(n for n in made["Plain"].node_tree.nodes if n.bl_idname == "ShaderNodeBsdfPrincipled")
        color = bsdf.inputs["Base Color"]
        check(not color.is_linked and all(abs(a - b) < 1e-4 for a, b in zip(color.default_value, (0.1, 0.9, 0.2))),
              f"Plain's tint argument: {list(color.default_value)}")
    if "Textured" in made:
        images = [n.image for n in made["Textured"].node_tree.nodes if n.bl_idname == "ShaderNodeTexImage"]
        check(any(i is not None and os.path.basename(i.filepath) == "checker.png" for i in images),
              f"Textured's image argument: {[i and i.filepath for i in images]}")


def test_tables():
    """A module's constant table read at a run-time index is an N x 1 float image the material samples by texel,
    packed as OpenEXR so its entries (151, 160, ...) survive saving."""
    (hashed,) = imported(os.path.join(fixtures, "tables.mdl"), mode="LOBES")
    images = {n.image.name: n.image for n in hashed.node_tree.nodes
              if n.bl_idname == "ShaderNodeTexImage" and n.image is not None}
    tables = sorted(name for name in images if " table " in name)
    check(tables == ["hashed table 0", "hashed table 1"], f"table images: {sorted(images)}")
    # The index's `& 255` is a floored modulo by 256, not the zero an unsupported op reads as.
    modulos = [n for n in hashed.node_tree.nodes if n.bl_idname == "ShaderNodeMath" and n.operation == "FLOORED_MODULO"
               and abs(n.inputs[1].default_value - 256.0) < 1e-6]
    check(bool(modulos), "the index's & 255 is no floored modulo by 256")
    if "hashed table 0" not in images:
        return
    perm = images["hashed table 0"]
    check(tuple(perm.size) == (256, 1) and perm.is_float, f"permutation image: {tuple(perm.size)} float {perm.is_float}")
    check(round(perm.pixels[0]) == 151 and round(perm.pixels[4]) == 160, f"entries: {list(perm.pixels[:8])}")
    path = os.path.join(scratch, "tables.blend")
    bpy.ops.wm.save_as_mainfile(filepath=path, copy=True)
    bpy.ops.wm.open_mainfile(filepath=path)
    reloaded = bpy.data.images["hashed table 0"]
    check(reloaded.packed_file is not None, "the table image is not packed")
    check(round(reloaded.pixels[0]) == 151 and round(reloaded.pixels[4 * 255]) == 180,
          f"entries after reload: {list(reloaded.pixels[:8])}")


test_import_principled()
test_import_lobes()
test_usd_import()
tinted = test_search_paths()
test_rebuild(tinted)
test_fill()
test_export_one()
test_export_several()
test_tables()  # last: it reloads the file
shutil.rmtree(scratch, ignore_errors=True)
if failures:
    sys.exit(f"{len(failures)} failures")
print("all MDL tests passed")
