"""Shadewire MaterialX: MaterialX materials as Blender material node trees, for Cycles and EEVEE.

Shadewire compiles each material to its IR; this add-on turns the IR into nodes. The pattern graph becomes Math,
texture and geometry nodes. Then, by default (Principled BSDF), a named model (standard_surface, OpenPBR,
glTF PBR) drives one Principled BSDF and a raw BSDF graph becomes one BSDF node per lobe; with MaterialX Lobes
every material opens into its lobes, as MaterialX defines them. File > Import > MaterialX
creates the document's materials; "Fill Materials from MaterialX" (Material properties, and the 3D view's
Object menu) rebuilds the materials already in the file whose names match the document's, such as the empty
ones Blender's USD importer makes for materials bound from a `.mtlx` reference.
"""

import os

import bpy
from bpy.props import BoolProperty, EnumProperty, StringProperty
from bpy_extras.io_utils import ImportHelper

from . import builder, compiler


def configured_executable(context):
    """The compiler path from the preferences; empty when unset or when the add-on is not installed."""
    addon = context.preferences.addons.get(__package__)
    return addon.preferences.executable if addon else ""


class ShadewirePreferences(bpy.types.AddonPreferences):
    bl_idname = __package__

    executable: StringProperty(
        name="shadewire executable",
        description="A shadewire command-line compiler to use instead of the bundled shadewire module (the "
        "module is used when this is empty and it is installed; else the add-on's bin folder, then PATH)",
        subtype="FILE_PATH",
    )

    def draw(self, context):
        self.layout.prop(self, "executable")


MODES = [
    ("PRINCIPLED", "Principled BSDF", "Named models (standard_surface, OpenPBR, glTF PBR) drive one Principled BSDF, "
     "small and editable, for Cycles and EEVEE; raw BSDF graphs become their lobes"),
    ("LOBES", "MaterialX Lobes", "Every material as its lobes, one BSDF node each, layered and mixed as MaterialX "
     "defines them (closest to MaterialX; more nodes than EEVEE evaluates)"),
]


def mode_property():
    return EnumProperty(name="Shading", items=MODES, default="PRINCIPLED")


def build(operator, context, mtlx, name, material, editable, mode):
    """Compiles `name` of `mtlx` into `material`; reports warnings. False on failure."""
    try:
        doc = compiler.backend(configured_executable(context)).compile(mtlx, name, editable, mode)
    except compiler.CompileError as e:
        operator.report({"ERROR"}, f"{name or os.path.basename(mtlx)}: {e}")
        return False
    warnings = builder.build_material(material, doc, os.path.dirname(mtlx))
    for w in warnings:
        operator.report({"WARNING"}, f"{material.name}: {w}")
    material["shadewire_source"] = mtlx
    material["shadewire_material"] = name or ""
    material["shadewire_mode"] = mode
    return True


class SHADEWIRE_OT_import(bpy.types.Operator, ImportHelper):
    """Create Blender materials from a MaterialX document"""

    bl_idname = "shadewire.import_mtlx"
    bl_label = "Import MaterialX"
    bl_options = {"REGISTER", "UNDO"}

    filename_ext = ".mtlx"
    filter_glob: StringProperty(default="*.mtlx", options={"HIDDEN"})
    editable: BoolProperty(
        name="Editable Values",
        description="Keep authored values as Value and RGB nodes (else they are baked in, and lobes they turn off "
        "are left out)",
        default=False,
    )
    mode: mode_property()
    assign: BoolProperty(name="Assign to Selected", description="Give the first material to the selected objects")

    def execute(self, context):
        try:
            names = compiler.backend(configured_executable(context)).material_names(self.filepath)
        except compiler.CompileError as e:
            self.report({"ERROR"}, str(e))
            return {"CANCELLED"}
        made = []
        for name in names:
            material = bpy.data.materials.new(name)
            if build(self, context, self.filepath, name, material, self.editable, self.mode):
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


class SHADEWIRE_OT_fill(bpy.types.Operator, ImportHelper):
    """Rebuild the materials whose names match a MaterialX document's materials"""

    bl_idname = "shadewire.fill_materials"
    bl_label = "Fill Materials from MaterialX"
    bl_options = {"REGISTER", "UNDO"}

    filename_ext = ".mtlx"
    filter_glob: StringProperty(default="*.mtlx", options={"HIDDEN"})
    editable: BoolProperty(name="Editable Values", default=False)
    mode: mode_property()

    def execute(self, context):
        try:
            names = compiler.backend(configured_executable(context)).material_names(self.filepath)
        except compiler.CompileError as e:
            self.report({"ERROR"}, str(e))
            return {"CANCELLED"}
        filled = 0
        for name in names:
            material = bpy.data.materials.get(name)
            if material is not None and build(self, context, self.filepath, name, material, self.editable, self.mode):
                filled += 1
        self.report({"INFO"}, f"filled {filled} materials ({len(names)} in the document)")
        return {"FINISHED"}


class SHADEWIRE_OT_rebuild(bpy.types.Operator):
    """Compile the active material again from the MaterialX document it came from"""

    bl_idname = "shadewire.rebuild_material"
    bl_label = "Rebuild from MaterialX"
    bl_options = {"REGISTER", "UNDO"}

    editable: BoolProperty(name="Editable Values", default=False)

    @classmethod
    def poll(cls, context):
        m = getattr(context, "material", None)
        return m is not None and "shadewire_source" in m

    def execute(self, context):
        m = context.material
        mode = m.get("shadewire_mode", "PRINCIPLED")
        ok = build(self, context, m["shadewire_source"], m["shadewire_material"] or None, m, self.editable, mode)
        return {"FINISHED"} if ok else {"CANCELLED"}


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
        self.layout.operator(SHADEWIRE_OT_fill.bl_idname)


def menu_import(self, context):
    self.layout.operator(SHADEWIRE_OT_import.bl_idname, text="MaterialX (Shadewire) (.mtlx)")


def menu_object(self, context):
    self.layout.operator(SHADEWIRE_OT_fill.bl_idname)


CLASSES = (ShadewirePreferences, SHADEWIRE_OT_import, SHADEWIRE_OT_fill, SHADEWIRE_OT_rebuild, SHADEWIRE_PT_material)


def register():
    for cls in CLASSES:
        bpy.utils.register_class(cls)
    bpy.types.TOPBAR_MT_file_import.append(menu_import)
    bpy.types.VIEW3D_MT_object.append(menu_object)


def unregister():
    bpy.types.VIEW3D_MT_object.remove(menu_object)
    bpy.types.TOPBAR_MT_file_import.remove(menu_import)
    for cls in reversed(CLASSES):
        bpy.utils.unregister_class(cls)
