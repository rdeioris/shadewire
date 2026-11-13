# SPDX-FileCopyrightText: 2026 Roberto De Ioris
# SPDX-License-Identifier: GPL-3.0-or-later

"""Shadewire MaterialX: MaterialX materials as Blender material node trees, for Cycles and EEVEE.

Shadewire compiles each material to its IR; this add-on turns the IR into nodes. The pattern graph becomes Math,
texture and geometry nodes. Then, by default (Principled BSDF), a named model (standard_surface, OpenPBR,
glTF PBR) drives one Principled BSDF and a raw BSDF graph becomes one BSDF node per lobe; with MaterialX Lobes
every material opens into its lobes, as MaterialX defines them. File > Import > MaterialX
creates the document's materials, File > Import > MDL a module's (distilled to one Principled BSDF, or as its
lobes); "Fill Materials from MaterialX" (Material properties, and the 3D view's
Object menu) rebuilds the materials already in the file whose names match the document's. Blender's USD importer
calls the add-on for the MaterialX materials of a scene (`usd.py`), so they arrive built; Fill is for scenes
imported without it. File > Export > MaterialX writes materials as one MaterialX document through Shadewire, and
the USD exporter's "Shadewire MaterialX" option (`usd_export.py`) references such a document from the exported
materials. File > Export > MDL writes each material as an MDL module. File > Export and Import > Material Graph
(Shadewire JSON) save and load materials as a JSON document of their node trees (`graph.py`).
"""

import array
import os
import shutil

import bpy
from bpy.props import BoolProperty, EnumProperty, StringProperty
from bpy_extras.io_utils import ExportHelper, ImportHelper

from . import builder, compiler, graph, usd

GRAPH_LABEL = "Material Graph (Shadewire JSON) (.json)"


def configured_executable(context):
    """The compiler path from the preferences; empty when unset or when the add-on is not installed."""
    addon = context.preferences.addons.get(__package__)
    return addon.preferences.executable if addon else ""


def configured_mdl_paths(context):
    """The MDL search path from the preferences; empty when unset or when the add-on is not installed."""
    addon = context.preferences.addons.get(__package__)
    return compiler.split_paths(addon.preferences.mdl_paths) if addon else []


def make_backend(context, executable="", mdl_paths=()):
    """The compiler: `executable` over the preferences' one; `mdl_paths` searched before the preferences'."""
    paths = list(mdl_paths) + [p for p in configured_mdl_paths(context) if p not in mdl_paths]
    return compiler.backend(executable or configured_executable(context), paths)


MODES = [
    ("PRINCIPLED", "Principled BSDF", "Named models (standard_surface, OpenPBR, glTF PBR) drive one Principled BSDF, "
     "small and editable, for Cycles and EEVEE; raw BSDF graphs become their lobes"),
    ("LOBES", "MaterialX Lobes", "Every material as its lobes, one BSDF node each, layered and mixed as MaterialX "
     "defines them (closest to MaterialX; more nodes than EEVEE evaluates)"),
]


MDL_MODES = [
    ("PRINCIPLED", "Principled BSDF", "Distill each material's lobes to one Principled BSDF (OpenPBR's parameters), "
     "small and editable, for Cycles and EEVEE"),
    ("LOBES", "MDL Lobes", "Every material as its lobes, one BSDF node each, layered and mixed as its df tree is "
     "(closest to MDL; more nodes than EEVEE evaluates)"),
]


def mode_property():
    return EnumProperty(name="Shading", items=MODES, default="PRINCIPLED")


def mdl_mode_property():
    return EnumProperty(name="Shading", items=MDL_MODES, default="PRINCIPLED")


def mdl_paths_property():
    return StringProperty(
        name="Search Paths",
        description="Folders where MDL imports find modules, separated by ';' (searched before the preferences'; "
        "the standard modules, ::base and ::nvidia::core_definitions are built in)",
    )


class ShadewirePreferences(bpy.types.AddonPreferences):
    bl_idname = __package__

    executable: StringProperty(
        name="shadewire executable",
        description="A shadewire command-line compiler to use instead of the bundled shadewire module (the "
        "module is used when this is empty and it is installed; else the add-on's bin folder, then PATH)",
        subtype="FILE_PATH",
    )

    usd_import: BoolProperty(
        name="Build MaterialX materials of USD imports",
        description="Blender's USD importer hands the add-on the materials it imports with a MaterialX surface, "
        "which it builds as File > Import > MaterialX does",
        default=True,
    )
    usd_mode: mode_property()
    usd_editable: BoolProperty(
        name="Editable Values",
        description="Keep the USD materials' authored values as Value and RGB nodes",
        default=False,
    )
    mdl_paths: StringProperty(
        name="MDL search paths",
        description="Folders where MDL imports find modules, separated by ';' (the standard modules, ::base and "
        "::nvidia::core_definitions are built in)",
    )

    def draw(self, context):
        self.layout.prop(self, "executable")
        self.layout.prop(self, "mdl_paths")
        self.layout.prop(self, "usd_import")
        row = self.layout.row()
        row.active = self.usd_import
        row.prop(self, "usd_mode")
        row.prop(self, "usd_editable")


def build(operator, context, source, name, material, editable, mode, compiled=None, mdl_paths=()):
    """Compiles `name` of `source` (a MaterialX document, or an MDL module by extension) into `material` with
    `compiled` (None: the preferences' compiler); reports warnings. False on failure. An MDL material keeps the
    search path it was built with, for Rebuild."""
    try:
        compiled = compiled or make_backend(context, mdl_paths=mdl_paths)
        doc = compiled.compile(source, name, editable, mode)
    except compiler.CompileError as e:
        operator.report({"ERROR"}, f"{name or os.path.basename(source)}: {e}")
        return False
    warnings = builder.build_material(material, doc, os.path.dirname(source))
    for w in warnings:
        operator.report({"WARNING"}, f"{material.name}: {w}")
    material["shadewire_source"] = source
    material["shadewire_material"] = name or ""
    material["shadewire_mode"] = mode
    if compiler.is_mdl(source):
        material["shadewire_mdl_paths"] = ";".join(mdl_paths)
    return True


class ImportMaterials(ImportHelper):
    """File > Import of a MaterialX document or an MDL module: one Blender material per material of the file. Not an
    operator itself (Blender cannot register an operator class that another one subclasses)."""

    bl_options = {"REGISTER", "UNDO"}

    editable: BoolProperty(
        name="Editable Values",
        description="Keep authored values as Value and RGB nodes (else they are baked in, and lobes they turn off "
        "are left out)",
        default=False,
    )
    assign: BoolProperty(name="Assign to Selected", description="Give the first material to the selected objects")
    # A test hook: the compiler to use instead of the preferences'; an installed extension leaves it empty.
    executable: StringProperty(options={"HIDDEN"})

    def search_paths(self):
        return []

    def execute(self, context):
        mdl_paths = self.search_paths()
        try:
            compiled = make_backend(context, self.executable, mdl_paths)
            names = compiled.material_names(self.filepath)
        except compiler.CompileError as e:
            self.report({"ERROR"}, str(e))
            return {"CANCELLED"}
        made = []
        for name in names:
            material = bpy.data.materials.new(name)
            if build(self, context, self.filepath, name, material, self.editable, self.mode, compiled, mdl_paths):
                made.append(material)
            else:
                bpy.data.materials.remove(material)
        if self.assign and made:
            for obj in context.selected_objects:
                if obj.type == "MESH":
                    if obj.material_slots:
                        obj.material_slots[0].material = made[0]
                    else:
                        obj.data.materials.append(made[0])
        self.report({"INFO"}, f"imported {len(made)} of {len(names)} materials")
        return {"FINISHED"}


class SHADEWIRE_OT_import(bpy.types.Operator, ImportMaterials):
    """Create Blender materials from a MaterialX document"""

    bl_idname = "shadewire.import_mtlx"
    bl_label = "Import MaterialX"

    filename_ext = ".mtlx"
    filter_glob: StringProperty(default="*.mtlx", options={"HIDDEN"})
    mode: mode_property()


class SHADEWIRE_OT_import_mdl(bpy.types.Operator, ImportMaterials):
    """Create Blender materials from the exported materials of an MDL module"""

    bl_idname = "shadewire.import_mdl"
    bl_label = "Import MDL"

    filename_ext = ".mdl"
    filter_glob: StringProperty(default="*.mdl", options={"HIDDEN"})
    mode: mdl_mode_property()
    mdl_paths: mdl_paths_property()

    def search_paths(self):
        return compiler.split_paths(self.mdl_paths)


class SHADEWIRE_OT_fill(bpy.types.Operator, ImportHelper):
    """Rebuild the materials whose names match a MaterialX document's or an MDL module's materials"""

    bl_idname = "shadewire.fill_materials"
    bl_label = "Fill Materials from MaterialX"
    bl_options = {"REGISTER", "UNDO"}

    filename_ext = ".mtlx"
    filter_glob: StringProperty(default="*.mtlx;*.mdl", options={"HIDDEN"})
    editable: BoolProperty(name="Editable Values", default=False)
    mode: mode_property()
    mdl_paths: mdl_paths_property()
    # A test hook: the compiler to use instead of the preferences'; an installed extension leaves it empty.
    executable: StringProperty(options={"HIDDEN"})

    def execute(self, context):
        mdl_paths = compiler.split_paths(self.mdl_paths)
        try:
            compiled = make_backend(context, self.executable, mdl_paths)
            names = compiled.material_names(self.filepath)
        except compiler.CompileError as e:
            self.report({"ERROR"}, str(e))
            return {"CANCELLED"}
        filled = 0
        for name in names:
            material = bpy.data.materials.get(name)
            if material is not None and build(self, context, self.filepath, name, material, self.editable, self.mode,
                                              compiled, mdl_paths):
                filled += 1
        self.report({"INFO"}, f"filled {filled} materials ({len(names)} in the document)")
        return {"FINISHED"}


class SHADEWIRE_OT_rebuild(bpy.types.Operator):
    """Compile the active material again from the MaterialX document or MDL module it came from"""

    bl_idname = "shadewire.rebuild_material"
    bl_label = "Rebuild from Source"
    bl_options = {"REGISTER", "UNDO"}

    editable: BoolProperty(name="Editable Values", default=False)
    # A test hook: the compiler to use instead of the preferences'; an installed extension leaves it empty.
    executable: StringProperty(options={"HIDDEN"})

    @classmethod
    def poll(cls, context):
        m = getattr(context, "material", None)
        return m is not None and "shadewire_source" in m

    def execute(self, context):
        m = context.material
        mode = m.get("shadewire_mode", "PRINCIPLED")
        mdl_paths = compiler.split_paths(m.get("shadewire_mdl_paths", ""))
        try:
            compiled = make_backend(context, self.executable, mdl_paths)
        except compiler.CompileError as e:
            self.report({"ERROR"}, str(e))
            return {"CANCELLED"}
        ok = build(self, context, m["shadewire_source"], m["shadewire_material"] or None, m, self.editable, mode,
                   compiled, mdl_paths)
        return {"FINISHED"} if ok else {"CANCELLED"}


def scope_materials(context, scope):
    """The materials a Save Graph scope names, in order and without repeats."""
    if scope == "ALL":
        return list(bpy.data.materials)
    if scope == "ACTIVE":
        obj = context.object
        material = obj.active_material if obj is not None else None
        return [material] if material is not None else []
    found = []
    for obj in context.selected_objects:
        for slot in obj.material_slots:
            if slot.material is not None and slot.material not in found:
                found.append(slot.material)
    return found


PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


class TextureFolder:
    """The textures folder of one export: each source is written once, under its own file name unless another source
    of this export took that name first (then `name.1.ext`, `name.2.ext`, ...). A file already in the folder (an
    export again over the same folder) is kept where it is, its name taken before any other source is placed."""

    def __init__(self, folder, entries):
        self.folder = folder
        self.paths = {}
        self.taken = set()
        for entry in entries:
            source = file_source(entry, bpy.data.images.get(entry["name"]))
            if source and os.path.normcase(source) == os.path.normcase(
                    os.path.abspath(os.path.join(folder, os.path.basename(source)))):
                self.taken.add(os.path.normcase(source))
                self.paths[os.path.normcase(source)] = source

    def place(self, key, stem, ext):
        """The path for the source `key` and whether it still has to be written there."""
        if key in self.paths:
            return self.paths[key], False
        path = os.path.join(self.folder, stem + ext)
        number = 0
        while os.path.normcase(path) in self.taken:
            number += 1
            path = os.path.join(self.folder, f"{stem}.{number}{ext}")
        self.taken.add(os.path.normcase(path))
        self.paths[key] = path
        return path, True


def packed_name(image, data):
    """The file name for a packed image's bytes: its original file's name; with none, the image's name and `.png`
    when the bytes are a PNG (Blender packs generated and painted images as PNG), else `.bin`."""
    stem, ext = os.path.splitext(bpy.path.basename(image.filepath))
    if stem and ext:
        return stem, ext
    return bpy.path.clean_name(image.name), ".png" if data.startswith(PNG_SIGNATURE) else ".bin"


def file_source(entry, image):
    """The absolute path of the file an image is exported from by copying it, or None when it is not: an image edited
    in memory (dirty) or packed is exported from what Blender holds, not from its file."""
    source = entry["filepath"]
    if image is not None and (image.is_dirty or image.packed_file is not None):
        return None
    if entry["source"] != "FILE" or not source or not os.path.isfile(source):
        return None
    return os.path.abspath(source)


def pixels_stem(image):
    """The file name stem of an image saved from its pixels: its file's, else its name."""
    return os.path.splitext(bpy.path.basename(image.filepath))[0] or bpy.path.clean_name(image.name)


def saved_format(image):
    """The file format and extension an image without a file is saved in: OpenEXR for a float image, whose pixels an
    8-bit PNG would hold encoded for its own colorspace instead of the image's, else PNG."""
    return ("OPEN_EXR", ".exr") if image.is_float else ("PNG", ".png")


def save_pixels(image, path, file_format):
    """Save the image's pixels the way `Image.save` writes them (no view transform, which `save_render` would apply),
    through a copy so the image keeps its own path and format. A copy starts from the generated pixels, not the
    painted ones, so the pixels are handed over. The copy is Non-Color: Blender encodes a float image's pixels for
    sRGB when it saves them (an OpenEXR file too), and Non-Color writes them as they are, which the document's
    colorspace then reads back."""
    pixels = array.array("f", [0.0]) * len(image.pixels)
    if not pixels:
        raise OSError("the image has no pixels")
    image.pixels.foreach_get(pixels)
    copy = image.copy()
    try:
        copy.colorspace_settings.name = "Non-Color"
        copy.pixels.foreach_set(pixels)
        copy.filepath_raw = path
        copy.file_format = file_format
        copy.save()
    finally:
        bpy.data.images.remove(copy)


def copy_texture(operator, entry, textures):
    """Put one dumped image into the export's textures folder and point its entry there, its pixels as they are: an
    image edited in memory is saved from its pixels, a packed image's bytes are written as they are, a file is
    copied (or kept, when it is already in the folder), anything else is saved from its pixels as PNG (OpenEXR when
    they are floats)."""
    image = bpy.data.images.get(entry["name"])
    source = file_source(entry, image)
    try:
        if image is not None and image.packed_file is not None and not image.is_dirty:
            data = image.packed_file.data
            target, new = textures.place(("packed", image.name), *packed_name(image, data))
            if new:
                with open(target, "wb") as f:
                    f.write(data)
        elif source is not None:
            stem, ext = os.path.splitext(os.path.basename(source))
            target, new = textures.place(os.path.normcase(source), stem, ext)
            if new and not (os.path.exists(target) and os.path.samefile(source, target)):
                shutil.copy2(source, target)
        elif image is not None:
            file_format, ext = saved_format(image)
            target, new = textures.place(("image", image.name), pixels_stem(image), ext)
            if new:
                save_pixels(image, target, file_format)
        else:
            raise OSError("the image is not in the file")
    except (OSError, RuntimeError) as e:
        operator.report({"WARNING"}, f"image {entry['name']}: not written: {e}")
        return
    entry["filepath"] = os.path.abspath(target).replace("\\", "/")


def copy_images(operator, doc, folder):
    """Put every image of the dumped document into the textures `folder` (made when there are any), the entries
    pointing there."""
    if not doc["images"]:
        return
    os.makedirs(folder, exist_ok=True)
    textures = TextureFolder(folder, doc["images"])
    for entry in doc["images"]:
        copy_texture(operator, entry, textures)


def relative_path(path, folder):
    """`path` relative to `folder` with forward slashes when it lies inside it; else as it is (with forward slashes):
    a file that could not be copied, on another drive, or no file at all."""
    if not path:
        return path
    try:
        relative = os.path.relpath(path, folder)
    except ValueError:
        return path.replace("\\", "/")
    if relative.split(os.sep)[0] == os.pardir:
        return path.replace("\\", "/")
    return relative.replace("\\", "/")


class SHADEWIRE_OT_export_graph(bpy.types.Operator, ExportHelper):
    """Save materials as a Shadewire graph document (JSON): their node trees, groups and image references"""

    bl_idname = "shadewire.export_graph"
    bl_label = "Save Material Graph"
    bl_options = {"REGISTER"}

    filename_ext = ".json"
    filter_glob: StringProperty(default="*.json", options={"HIDDEN"})
    scope: EnumProperty(
        name="Materials",
        items=[
            ("SELECTED", "Selected Objects", "The materials of the selected objects"),
            ("ALL", "All Materials", "Every material in the file"),
            ("ACTIVE", "Active Material", "The active object's active material"),
        ],
        default="SELECTED",
    )
    copy_textures: BoolProperty(
        name="Copy Textures",
        description="Write the images into textures/ beside the file and point the document at them",
        default=False,
    )

    def execute(self, context):
        materials = scope_materials(context, self.scope)
        if not materials:
            self.report({"ERROR"}, "no materials to save")
            return {"CANCELLED"}
        doc = graph.dump(materials)
        try:
            if self.copy_textures:
                copy_images(self, doc, os.path.join(os.path.dirname(self.filepath), "textures"))
            text = graph.to_text(doc)
            with open(self.filepath, "w", encoding="utf-8", newline="\n") as f:
                f.write(text)
        except (OSError, ValueError) as e:
            self.report({"ERROR"}, str(e))
            return {"CANCELLED"}
        self.report({"INFO"}, f"exported {len(materials)} materials ({len(doc['images'])} images)")
        return {"FINISHED"}


class ExportMaterials(ExportHelper):
    """What File > Export to MaterialX and to MDL share: the materials in scope and their textures. Not an operator
    itself (Blender cannot register an operator class that another one subclasses)."""

    bl_options = {"REGISTER"}

    scope: EnumProperty(
        name="Materials",
        items=[
            ("SELECTED", "Selected Objects", "The materials of the selected objects"),
            ("ALL", "All Materials", "Every material in the file"),
            ("ACTIVE", "Active Material", "The active object's active material"),
        ],
        default="SELECTED",
    )
    copy_textures: BoolProperty(
        name="Copy Textures",
        description="Write the images into textures/ beside the file and reference them there",
        default=True,
    )
    # A test hook: the compiler to use instead of the preferences'; an installed extension leaves it empty.
    executable: StringProperty(options={"HIDDEN"})

    def copy_images(self, doc):
        """Write the document's images beside the file when asked, then make the paths that lead there relative to
        the file; an image that could not be copied keeps the path it had."""
        if not self.copy_textures:
            return
        folder = os.path.dirname(self.filepath)
        copy_images(self, doc, os.path.join(folder, "textures"))
        for entry in doc["images"]:
            entry["filepath"] = relative_path(entry["filepath"], folder)


class SHADEWIRE_OT_export_mtlx(bpy.types.Operator, ExportMaterials):
    """Write materials as one MaterialX document, compiled by Shadewire from their node trees"""

    bl_idname = "shadewire.export_mtlx"
    bl_label = "Export MaterialX"

    filename_ext = ".mtlx"
    filter_glob: StringProperty(default="*.mtlx", options={"HIDDEN"})
    inline: BoolProperty(
        name="Inline Nodes",
        description="Write library nodes expanded into the nodes they are built from",
        default=False,
    )

    def compile_materials(self, context, materials, doc):
        """The MaterialX text of every material and the warnings (each led by its material's name), each material
        named after Blender's."""
        compiled = make_backend(context, self.executable)
        text = graph.to_text(doc)
        texts, reports = [], []
        for material in materials:
            try:
                document, warnings = compiled.to_mtlx(text, material.name, material.name, self.inline)
            except compiler.CompileError as e:
                raise compiler.CompileError(f"{material.name}: {e}") from e
            texts.append(document)
            reports += [f"{material.name}: {w}" for w in warnings]
        return compiler.merge_documents(texts), reports

    def execute(self, context):
        materials = scope_materials(context, self.scope)
        if not materials:
            self.report({"ERROR"}, "no materials to export")
            return {"CANCELLED"}
        doc = graph.dump(materials)
        try:
            self.copy_images(doc)
            text, warnings = self.compile_materials(context, materials, doc)
            with open(self.filepath, "w", encoding="utf-8", newline="\n") as f:
                f.write(text)
        except (compiler.CompileError, OSError, ValueError) as e:
            self.report({"ERROR"}, str(e))
            return {"CANCELLED"}
        for w in warnings:
            self.report({"WARNING"}, w)
        self.report({"INFO"}, f"exported {len(materials)} materials to {os.path.basename(self.filepath)}")
        return {"FINISHED"}


class SHADEWIRE_OT_export_mdl(bpy.types.Operator, ExportMaterials):
    """Write each material as an MDL 1.7 module, compiled by Shadewire from its node tree"""

    bl_idname = "shadewire.export_mdl"
    bl_label = "Export MDL"

    filename_ext = ".mdl"
    filter_glob: StringProperty(default="*.mdl", options={"HIDDEN"})

    def draw(self, context):
        self.layout.prop(self, "scope")
        self.layout.prop(self, "copy_textures")
        self.layout.label(text="Several materials: one module each, named after it")

    def module_paths(self, materials):
        """Where each material's module goes: the chosen file for a single material, else one module per material
        beside it, named after the material (as an MDL identifier, `_2`, `_3`, ... when two come out alike)."""
        if len(materials) == 1:
            return [self.filepath]
        folder, taken = os.path.dirname(self.filepath), set()
        paths = []
        for material in materials:
            name = compiler.unique_name(material.name, taken)
            taken.add(name)
            paths.append(os.path.join(folder, name + ".mdl"))
        return paths

    def execute(self, context):
        materials = scope_materials(context, self.scope)
        if not materials:
            self.report({"ERROR"}, "no materials to export")
            return {"CANCELLED"}
        doc = graph.dump(materials)
        paths = self.module_paths(materials)
        try:
            self.copy_images(doc)
            compiled = make_backend(context, self.executable)
            text = graph.to_text(doc)
            modules, warnings = [], []
            for material in materials:
                try:
                    code, notes = compiled.to_mdl(text, material.name, material.name)
                except compiler.CompileError as e:
                    raise compiler.CompileError(f"{material.name}: {e}") from e
                modules.append(code)
                warnings += [f"{material.name}: {w}" for w in notes]
            for path, code in zip(paths, modules):
                with open(path, "w", encoding="utf-8", newline="\n") as f:
                    f.write(code)
        except (compiler.CompileError, OSError, ValueError) as e:
            self.report({"ERROR"}, str(e))
            return {"CANCELLED"}
        for w in warnings:
            self.report({"WARNING"}, w)
        self.report({"INFO"}, f"exported {len(materials)} materials as {len(paths)} MDL modules")
        return {"FINISHED"}


class SHADEWIRE_OT_import_graph(bpy.types.Operator, ImportHelper):
    """Create materials from a Shadewire graph document (JSON)"""

    bl_idname = "shadewire.import_graph"
    bl_label = "Load Material Graph"
    bl_options = {"REGISTER", "UNDO"}

    filename_ext = ".json"
    filter_glob: StringProperty(default="*.json", options={"HIDDEN"})
    replace: BoolProperty(
        name="Replace Existing",
        description="Rebuild a material of the same name instead of making a new one",
        default=False,
    )

    def execute(self, context):
        try:
            with open(self.filepath, encoding="utf-8") as f:
                doc = graph.from_text(f.read())
        except (OSError, ValueError) as e:
            self.report({"ERROR"}, f"{os.path.basename(self.filepath)}: {e}")
            return {"CANCELLED"}
        try:
            materials, warnings = graph.load(doc, replace=self.replace)
        except (KeyError, TypeError, ValueError) as e:
            self.report({"ERROR"}, f"{os.path.basename(self.filepath)}: malformed document: {type(e).__name__} {e}")
            return {"CANCELLED"}
        for w in warnings:
            self.report({"WARNING"}, w)
        self.report({"INFO"}, f"imported {len(materials)} materials")
        return {"FINISHED"}


class SHADEWIRE_PT_material(bpy.types.Panel):
    bl_label = "Shadewire MaterialX"
    bl_space_type = "PROPERTIES"
    bl_region_type = "WINDOW"
    bl_context = "material"

    def draw(self, context):
        m = getattr(context, "material", None)
        if m is not None and "shadewire_source" in m:
            self.layout.label(text=f"{os.path.basename(m['shadewire_source'])}: {m['shadewire_material']}")
            self.layout.operator(SHADEWIRE_OT_rebuild.bl_idname)
        row = self.layout.row()
        row.operator(SHADEWIRE_OT_export_graph.bl_idname, text="Save Graph").scope = "ACTIVE"
        row.operator(SHADEWIRE_OT_import_graph.bl_idname, text="Load Graph")
        self.layout.operator(SHADEWIRE_OT_fill.bl_idname)


class SHADEWIRE_PT_usd_export(bpy.types.Panel):
    bl_space_type = "FILE_BROWSER"
    bl_region_type = "TOOL_PROPS"
    bl_parent_id = "FILE_PT_operator"
    bl_label = "Shadewire MaterialX"

    @classmethod
    def poll(cls, context):
        operator = getattr(context.space_data, "active_operator", None)
        return operator is not None and operator.bl_idname == "WM_OT_usd_export"

    def draw(self, context):
        self.layout.prop(context.window_manager, "shadewire_usd_materials")
        self.layout.label(text="Untick USD Preview Surface unless viewers without MaterialX need a fallback")


def menu_import(self, context):
    self.layout.operator(SHADEWIRE_OT_import.bl_idname, text="MaterialX (Shadewire) (.mtlx)")
    self.layout.operator(SHADEWIRE_OT_import_mdl.bl_idname, text="MDL (Shadewire) (.mdl)")
    self.layout.operator(SHADEWIRE_OT_import_graph.bl_idname, text=GRAPH_LABEL)


def menu_export(self, context):
    self.layout.operator(SHADEWIRE_OT_export_mtlx.bl_idname, text="MaterialX (Shadewire) (.mtlx)")
    self.layout.operator(SHADEWIRE_OT_export_mdl.bl_idname, text="MDL (Shadewire) (.mdl)")
    self.layout.operator(SHADEWIRE_OT_export_graph.bl_idname, text=GRAPH_LABEL)


def menu_object(self, context):
    self.layout.operator(SHADEWIRE_OT_fill.bl_idname)


CLASSES = (ShadewirePreferences, SHADEWIRE_OT_import, SHADEWIRE_OT_import_mdl, SHADEWIRE_OT_fill, SHADEWIRE_OT_rebuild,
           SHADEWIRE_OT_export_mtlx, SHADEWIRE_OT_export_mdl, SHADEWIRE_OT_export_graph, SHADEWIRE_OT_import_graph,
           SHADEWIRE_PT_material, SHADEWIRE_PT_usd_export, usd.ShadewireUSDHook)


def register():
    for cls in CLASSES:
        bpy.utils.register_class(cls)
    bpy.types.WindowManager.shadewire_usd_materials = BoolProperty(
        name="Shadewire MaterialX",
        description="Replace Blender's shader prims by a reference into a MaterialX document written beside the USD "
        "file, one surfacematerial per material (a .usdz is left to Blender)",
        default=True,
    )
    bpy.types.TOPBAR_MT_file_import.append(menu_import)
    bpy.types.TOPBAR_MT_file_export.append(menu_export)
    bpy.types.VIEW3D_MT_object.append(menu_object)


def unregister():
    bpy.types.VIEW3D_MT_object.remove(menu_object)
    bpy.types.TOPBAR_MT_file_export.remove(menu_export)
    bpy.types.TOPBAR_MT_file_import.remove(menu_import)
    del bpy.types.WindowManager.shadewire_usd_materials
    for cls in reversed(CLASSES):
        bpy.utils.unregister_class(cls)
