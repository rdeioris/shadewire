# SPDX-FileCopyrightText: 2026 Roberto De Ioris
# SPDX-License-Identifier: GPL-3.0-or-later

"""MaterialX materials of USD scenes, built by Blender's USD importer through a `bpy.types.USDHook`.

Blender composes a `.mtlx` reference (its usdMtlx plugin) into UsdShade materials whose `mtlx` surface output it
does not convert. The hook claims those materials and fills them. When a material comes whole from a `.mtlx`
layer (no USD layer overrides it or its shaders), that document is compiled by the material's name, read through
USD's resolver so a `.mtlx` packed in a `.usdz` works too. Otherwise the composed network is written back as a
MaterialX document: every Shader prim the surface depends on becomes a node (its `info:id` the nodedef, its
category from the shader registry), and each input is either a value or a connection to the shader output that
produces it, through node graphs and the material's interface. Textures go through the importer's
`import_texture`, which unpacks them from a `.usdz`.
"""

import os
from xml.sax.saxutils import quoteattr

import bpy
from pxr import Ar, Sdf, Sdr, Usd, UsdShade

from . import builder, compiler

# Used when the add-on is not installed (run from a source checkout): its preferences do not exist.
defaults = {"usd_import": True, "usd_mode": "PRINCIPLED", "usd_editable": False, "executable": ""}

# Sdf value type names as MaterialX types.
TYPES = {
    "bool": "boolean", "int": "integer", "float": "float", "double": "float", "half": "float",
    "float2": "vector2", "double2": "vector2", "half2": "vector2", "texCoord2f": "vector2",
    "float3": "vector3", "double3": "vector3", "vector3f": "vector3", "normal3f": "vector3", "point3f": "vector3",
    "color3f": "color3", "color4f": "color4", "float4": "vector4", "double4": "vector4",
    "matrix3d": "matrix33", "matrix4d": "matrix44", "string": "string", "token": "string", "asset": "filename",
}

TERMINALS = {"displacement": "displacementshader", "volume": "volumeshader", "light": "lightshader"}


def settings(context):
    """The hook's options: the add-on's preferences, or `defaults` when it is not installed."""
    addon = context.preferences.addons.get(__package__)
    if addon is None:
        return dict(defaults)
    p = addon.preferences
    return {"usd_import": p.usd_import, "usd_mode": p.usd_mode, "usd_editable": p.usd_editable,
            "executable": p.executable}


def is_mtlx_layer(layer):
    return layer.identifier.rstrip("]").lower().endswith(".mtlx")


def mtlx_source(material):
    """(the `.mtlx` layer, the material's name in it) when `material` and every prim under it compose from that
    layer alone, else None."""
    layers = set()
    for prim in Usd.PrimRange(material.GetPrim()):
        layers.update(spec.layer for spec in prim.GetPrimStack())
    if len(layers) != 1:
        return None
    layer = layers.pop()
    if not is_mtlx_layer(layer):
        return None
    spec = material.GetPrim().GetPrimStack()[0]
    return layer, spec.path.name


def read_layer(layer):
    """A layer's bytes as text, through USD's resolver (so from inside a package too)."""
    asset = Ar.GetResolver().OpenAsset(Ar.ResolvedPath(layer.identifier))
    if asset is None:
        raise compiler.CompileError(f"cannot read {layer.identifier}")
    return bytes(asset.GetBuffer()).decode("utf-8")


def mtlx_value(value):
    """A UsdShade value as a MaterialX value string."""
    if isinstance(value, Sdf.AssetPath):
        return value.resolvedPath or value.path
    if isinstance(value, bool):
        return "true" if value else "false"
    if hasattr(value, "__len__") and not isinstance(value, str):
        flat = []
        for item in value:
            flat.extend(item if hasattr(item, "__len__") else [item])
        return ", ".join(f"{float(x):g}" for x in flat)
    if isinstance(value, float):
        return f"{value:g}"
    return str(value)


class Writer:
    """The network under a UsdShade material as a MaterialX document with one material, every shader a
    top-level node named after its path."""

    def __init__(self, material):
        self.material = material
        self.names = {}
        self.nodes = []

    def name(self, path):
        if path not in self.names:
            base = "_".join(path.pathString.strip("/").split("/")) or "node"
            taken = set(self.names.values())
            self.names[path] = f"{base}_{len(self.names)}" if base in taken else base
        return self.names[path]

    def shader(self, prim):
        """The node for a Shader prim (written once); its name."""
        if prim.GetPath() in self.names:
            return self.names[prim.GetPath()]
        name = self.name(prim.GetPath())
        shader = UsdShade.Shader(prim)
        node_def = shader.GetIdAttr().Get() or ""
        outputs = shader.GetOutputs()
        out_type = output_type(shader, outputs[0]) if len(outputs) == 1 else "multioutput"
        lines = [f"<{category(node_def)} name={quoteattr(name)} type={quoteattr(out_type)} "
                 f"nodedef={quoteattr(node_def)}>"]
        for put in shader.GetInputs():
            line = self.input(put)
            if line:
                lines.append("  " + line)
        lines.append(f"</{category(node_def)}>")
        self.nodes.append("\n".join(lines))
        return name

    def input(self, put):
        """An `<input>` for a UsdShade input: the value or shader output producing it, None when none does."""
        name = put.GetBaseName()
        sources = put.GetValueProducingAttributes(shaderOutputsOnly=False)
        if not sources:
            return None
        attr = sources[0]
        owner = attr.GetPrim()
        if UsdShade.Output.IsOutput(attr) and owner.IsA(UsdShade.Shader):
            node = self.shader(owner)
            shader = UsdShade.Shader(owner)
            out_type = output_type(shader, UsdShade.Output(attr))
            extra = ""
            if len(shader.GetOutputs()) > 1:
                extra = f" output={quoteattr(UsdShade.Output(attr).GetBaseName())}"
            return f"<input name={quoteattr(name)} type={quoteattr(out_type)} nodename={quoteattr(node)}{extra} />"
        value = attr.Get()
        mtlx_type = TYPES.get(str(put.GetTypeName()))
        if value is None or mtlx_type is None:
            return None
        extra = ""
        colorspace = attr.GetColorSpace() if hasattr(attr, "GetColorSpace") else ""
        if colorspace:
            extra += f" colorspace={quoteattr(colorspace)}"
        return (f"<input name={quoteattr(name)} type={quoteattr(mtlx_type)} "
                f"value={quoteattr(mtlx_value(value))}{extra} />")

    def document(self):
        output = self.material.GetSurfaceOutput("mtlx")
        sources = output.GetValueProducingAttributes(shaderOutputsOnly=True)
        if not sources:
            raise compiler.CompileError(f"{self.material.GetPath()}: the mtlx surface output is not connected")
        surface = self.shader(sources[0].GetPrim())
        name = self.material.GetPrim().GetName()
        body = "\n".join(self.nodes)
        return (f'<?xml version="1.0"?>\n<materialx version="1.39">\n{body}\n'
                f'<surfacematerial name={quoteattr(name)} type="material">\n'
                f'  <input name="surfaceshader" type="surfaceshader" nodename={quoteattr(surface)} />\n'
                f"</surfacematerial>\n</materialx>\n")


def output_type(shader, output):
    """The MaterialX type of a shader's output. USD types closures as tokens; the shader registry knows them
    (`BSDF`, `EDF`, `VDF`, or `terminal`, a shader of the node's context)."""
    node = Sdr.Registry().GetShaderNodeByIdentifier(shader.GetIdAttr().Get() or "")
    prop = None
    if node is not None:
        # usdMtlx names a shader's only output after its type (`surface`) rather than the nodedef's (`out`).
        names = node.GetShaderOutputNames()
        prop = node.GetShaderOutput(output.GetBaseName() if output.GetBaseName() in names or len(names) != 1
                                    else names[0])
    if prop is not None:
        if prop.GetType() in ("BSDF", "EDF", "VDF"):
            return prop.GetType()
        if prop.GetType() == "terminal":
            return TERMINALS.get(node.GetContext(), "surfaceshader")
    return TYPES.get(str(output.GetTypeName()), "float")


def category(node_def):
    """A nodedef's node category: its family in the shader registry, else the name without `ND_` and types."""
    node = Sdr.Registry().GetShaderNodeByIdentifier(node_def)
    if node is not None and node.GetFamily():
        return node.GetFamily()
    return node_def.removeprefix("ND_").split("_")[0]


def network_document(material):
    """The material's composed network as MaterialX text."""
    return Writer(material).document()


def convert(import_context, bl_material, usd_material, options):
    """Compiles `usd_material` and builds it into `bl_material`; the warnings."""
    backend = compiler.backend(options["executable"])
    source = mtlx_source(usd_material)
    if source is not None:
        layer, name = source
        path = layer.realPath if os.path.isfile(layer.realPath) else None
        doc = backend.compile_text(read_layer(layer), path, name, options["usd_editable"], options["usd_mode"])
        anchor = layer
    else:
        name = usd_material.GetPrim().GetName()
        path = None
        doc = backend.compile_text(network_document(usd_material), None, name, options["usd_editable"],
                                   options["usd_mode"])
        anchor = usd_material.GetPrim().GetPrimStack()[0].layer

    def resolve(file):
        anchored = Sdf.ComputeAssetPathRelativeToLayer(anchor, file) if anchor else file
        local, _ = import_context.import_texture(anchored)
        return local

    warnings = builder.build_material(bl_material, doc, "", resolve)
    if path:
        bl_material["shadewire_source"] = path
        bl_material["shadewire_material"] = name
    else:
        for key in ("shadewire_source", "shadewire_material"):
            bl_material.pop(key, None)
    bl_material["shadewire_mode"] = options["usd_mode"]
    return warnings


class ShadewireUSDHook(bpy.types.USDHook):
    bl_idname = "shadewire_mtlx"
    bl_label = "Shadewire MaterialX"
    bl_description = "Builds the MaterialX materials of imported USD scenes with Shadewire"

    @staticmethod
    def material_import_poll(import_context, usd_material):
        return settings(bpy.context)["usd_import"] and bool(usd_material.GetSurfaceOutput("mtlx"))

    @staticmethod
    def on_material_import(import_context, bl_material, usd_material):
        try:
            warnings = convert(import_context, bl_material, usd_material, settings(bpy.context))
        except compiler.CompileError as e:
            print(f"Shadewire: {usd_material.GetPath()}: {e}")
            return False
        for w in warnings:
            print(f"Shadewire: {bl_material.name}: {w}")
        return True
