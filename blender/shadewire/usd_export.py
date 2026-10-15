# SPDX-FileCopyrightText: 2026 Roberto De Ioris
# SPDX-License-Identifier: GPL-3.0-or-later

"""MaterialX materials of USD exports, written through Blender's `bpy.types.USDHook` export callbacks.

Blender's USD exporter hands each material to the hook once its prims are written (`collect`): Shadewire writes the
material as a MaterialX document, its textures the ones Blender exports beside the USD file, and the material's
shader prims are removed, except for the UsdPreviewSurface network when one was asked for (a fallback for viewers
without MaterialX). When the stage is complete (`finish`), the documents are merged into one `.mtlx` beside the root
layer and each material prim references its material there, which USD's usdMtlx plugin composes. A `.usdz` cannot
carry a `.mtlx` (usdMtlx only reads), so its materials are left as Blender wrote them.
"""

import os

import bpy
from pxr import UsdShade

from . import compiler, graph, usd

# Root layer identifier -> Export. One export runs at a time; an entry left by a failed export is replaced by the
# next export of the same file.
exports = {}

# Blender's attributes for its own MaterialX network, which the referenced document replaces.
MTLX_PROPERTIES = ("outputs:mtlx:surface", "outputs:mtlx:displacement", "outputs:mtlx:volume", "config:mtlx:version")
# The universal render context's outputs, removed when no UsdPreviewSurface network stays.
PREVIEW_OUTPUTS = ("outputs:surface", "outputs:displacement", "outputs:volume")
# The placeholder shader Blender writes when it makes no UsdPreviewSurface network.
PLACEHOLDER = "previewShader"


class Export:
    """What one USD export collected: the materials' documents by prim path, and whether the target is a usdz."""

    def __init__(self, stage):
        self.stage, self.materials, self.usdz, self.reported = stage, {}, is_usdz(stage), False


def is_usdz(stage):
    """Whether the stage is written into a `.usdz`: Blender exports one through a stage in a `USDZ` temporary
    folder."""
    identifier = stage.GetRootLayer().identifier.replace("\\", "/")
    return identifier.lower().endswith(".usdz") or "/USDZ/" in identifier


def enabled():
    """The export dialog's Shadewire MaterialX option."""
    return bpy.context.window_manager.shadewire_usd_materials


def document_path(stage):
    """The `.mtlx` written beside the root layer, named after it."""
    return os.path.splitext(stage.GetRootLayer().realPath)[0] + ".mtlx"


def collect(context, bl_material, usd_material):
    """Writes `bl_material` as a MaterialX document for `finish` and strips Blender's shaders from `usd_material`; a
    material Shadewire cannot write is left as Blender wrote it."""
    if not enabled():
        return
    stage = context.get_stage()
    export = exports.get(stage.GetRootLayer().identifier)
    if export is None or export.stage != stage:
        export = exports[stage.GetRootLayer().identifier] = Export(stage)
    if export.usdz:
        if not export.reported:
            print("Shadewire: a .usdz cannot carry a .mtlx; materials are left as Blender wrote them")
            export.reported = True
        return
    # The exporter hands over the evaluated material, some of whose properties Blender 5.2 crashes reading
    # (`blend_method`); its original is the material as the user made it.
    material = bl_material.original
    doc = graph.dump([material])
    for entry in doc["images"]:
        image = bpy.data.images.get(entry["name"])
        if image is not None:
            entry["filepath"] = context.export_texture(image) or entry["filepath"]
    name = usd_material.GetPrim().GetName()
    try:
        backend = compiler.backend(usd.settings(bpy.context)["executable"])
        text, warnings = backend.to_mtlx(graph.to_text(doc), material.name, name)
    except compiler.CompileError as e:
        print(f"Shadewire: {bl_material.name}: {e}; left as Blender wrote it")
        return
    for w in warnings:
        print(f"Shadewire: {bl_material.name}: {w}")
    strip_blender_shaders(usd_material, has_preview_surface(usd_material))
    export.materials[usd_material.GetPath()] = (name, text)


def has_preview_surface(usd_material):
    """Whether the material's surface comes from a UsdPreviewSurface network (Blender's USD Preview Surface option)
    rather than its placeholder shader."""
    return any(prim.GetName() != PLACEHOLDER for prim in surface_shaders(usd_material))


def surface_shaders(usd_material):
    """The shader prims the universal surface output connects to."""
    output = usd_material.GetSurfaceOutput()
    if not output:
        return []
    sources, _ = output.GetConnectedSources()
    return [source.source.GetPrim() for source in sources]


def preview_network(usd_material):
    """The paths of the shader prims the universal surface and displacement outputs depend on."""
    found = set()
    pending = list(surface_shaders(usd_material))
    displacement = usd_material.GetDisplacementOutput()
    if displacement:
        pending += [source.source.GetPrim() for source in displacement.GetConnectedSources()[0]]
    while pending:
        prim = pending.pop()
        if prim.GetPath() in found or not prim.IsA(UsdShade.Shader):
            continue
        found.add(prim.GetPath())
        for put in UsdShade.Shader(prim).GetInputs():
            pending += [source.source.GetPrim() for source in put.GetConnectedSources()[0]]
    return found


def strip_blender_shaders(usd_material, keep_preview):
    """Removes the prims and outputs Blender wrote under the material, but for its UsdPreviewSurface network when
    `keep_preview`."""
    prim = usd_material.GetPrim()
    stage = prim.GetStage()
    kept = preview_network(usd_material) if keep_preview else set()
    for child in prim.GetChildren():
        if not any(path.HasPrefix(child.GetPath()) for path in kept):
            stage.RemovePrim(child.GetPath())
    names = MTLX_PROPERTIES if keep_preview else MTLX_PROPERTIES + PREVIEW_OUTPUTS
    for name in names:
        if prim.HasProperty(name):
            prim.RemoveProperty(name)


def finish(context):
    """Writes the collected materials as one `.mtlx` beside the root layer and references each from its prim."""
    stage = context.get_stage()
    export = exports.pop(stage.GetRootLayer().identifier, None)
    if export is None or export.usdz or not export.materials:
        return
    path = document_path(stage)
    basename = os.path.basename(path)
    try:
        text = compiler.merge_documents([text for _, text in export.materials.values()])
    except compiler.CompileError as e:
        print(f"Shadewire: cannot write {basename}: {e}; the stripped materials have no shader")
        return
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)
    for prim_path, (name, _) in export.materials.items():
        stage.GetPrimAtPath(prim_path).GetReferences().AddReference(f"./{basename}", f"/MaterialX/Materials/{name}")
    print(f"Shadewire: wrote {basename} with {len(export.materials)} materials")
