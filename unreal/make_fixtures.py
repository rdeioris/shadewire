"""Builds the fixture materials in the test project, under /Game/Fixtures, from scratch.

Run inside the editor:
    UnrealEditor-Cmd unreal/ShadewireTest/ShadewireTest.uproject -run=pythonscript -script=<abs>/unreal/make_fixtures.py
then dump them:
    UnrealEditor-Cmd unreal/ShadewireTest/ShadewireTest.uproject -run=ShadewireExport -folder=/Game/Fixtures
        -out=<abs>/unreal/fixtures/graphs
"""

import os
import struct
import tempfile
import zlib

import unreal

ROOT = "/Game/Fixtures"
TEXTURES = f"{ROOT}/Textures"

mel = unreal.MaterialEditingLibrary
eal = unreal.EditorAssetLibrary
tools = unreal.AssetToolsHelpers.get_asset_tools()
P = unreal.MaterialProperty


def png(path, width, height, pixel):
    """Writes an 8-bit RGBA PNG whose pixel (x, y) is `pixel(x, y)`."""
    rows = b"".join(
        b"\x00" + b"".join(bytes(pixel(x, y)) for x in range(width)) for y in range(height)
    )

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b""))


def import_texture(folder, name, pixel, srgb=True, compression=None):
    path = os.path.join(folder, f"{name}.png")
    png(path, 8, 8, pixel)
    task = unreal.AssetImportTask()
    task.filename = path
    task.destination_path = TEXTURES
    task.destination_name = name
    task.automated = True
    task.replace_existing = True
    task.save = False
    tools.import_asset_tasks([task])
    texture = unreal.load_asset(f"{TEXTURES}/{name}")
    texture.set_editor_property("srgb", srgb)
    if compression is not None:
        texture.set_editor_property("compression_settings", compression)
    return texture


def material(name, **settings):
    m = tools.create_asset(name, ROOT, unreal.Material, unreal.MaterialFactoryNew())
    for key, value in settings.items():
        m.set_editor_property(key, value)
    return m


def node(m, cls, x=0, y=0, **props):
    e = mel.create_material_expression(m, cls, x, y)
    for key, value in props.items():
        e.set_editor_property(key, value)
    return e


def fnode(f, cls, x=0, y=0, **props):
    e = mel.create_material_expression_in_function(f, cls, x, y)
    for key, value in props.items():
        e.set_editor_property(key, value)
    return e


def link(a, out, b, inp):
    if not mel.connect_material_expressions(a, out, b, inp):
        raise RuntimeError(f"could not link {a.get_name()}.{out!r} to {b.get_name()}.{inp!r}")


def pin(a, out, prop):
    if not mel.connect_material_property(a, out, prop):
        raise RuntimeError(f"could not link {a.get_name()}.{out!r} to {prop}")


def c(m, value, x=0, y=0):
    return node(m, unreal.MaterialExpressionConstant, x, y, r=value)


def c3(m, r, g, b, x=0, y=0):
    return node(m, unreal.MaterialExpressionConstant3Vector, x, y, constant=unreal.LinearColor(r, g, b, 1.0))


def finish(m):
    mel.recompile_material(m)
    eal.save_loaded_asset(m)


def constants():
    m = material("M_Constants")
    pin(c3(m, 0.8, 0.2, 0.1), "", P.MP_BASE_COLOR)
    pin(c(m, 0.0), "", P.MP_METALLIC)
    pin(c(m, 0.7), "", P.MP_SPECULAR)
    pin(c(m, 0.4), "", P.MP_ROUGHNESS)
    pin(c3(m, 0.0, 0.0, 2.0), "", P.MP_EMISSIVE_COLOR)
    finish(m)


def parameters():
    m = material("M_Parameters")
    tint = node(m, unreal.MaterialExpressionVectorParameter, parameter_name="Tint",
                default_value=unreal.LinearColor(0.5, 0.5, 0.5, 1.0))
    rough = node(m, unreal.MaterialExpressionScalarParameter, parameter_name="Roughness", default_value=0.5)
    metal = node(m, unreal.MaterialExpressionScalarParameter, parameter_name="Metallic", default_value=1.0)
    switch = node(m, unreal.MaterialExpressionStaticSwitchParameter, parameter_name="UseTint", default_value=False)
    link(tint, "", switch, "True")
    link(c3(m, 1.0, 1.0, 1.0), "", switch, "False")
    pin(switch, "", P.MP_BASE_COLOR)
    pin(rough, "", P.MP_ROUGHNESS)
    pin(metal, "", P.MP_METALLIC)
    finish(m)

    mi = tools.create_asset("MI_Parameters", ROOT, unreal.MaterialInstanceConstant,
                            unreal.MaterialInstanceConstantFactoryNew())
    mel.set_material_instance_parent(mi, m)
    mel.set_material_instance_vector_parameter_value(mi, "Tint", unreal.LinearColor(0.1, 0.6, 0.2, 1.0))
    mel.set_material_instance_scalar_parameter_value(mi, "Roughness", 0.25)
    mel.set_material_instance_static_switch_parameter_value(mi, "UseTint", True)
    mel.update_material_instance(mi)
    eal.save_loaded_asset(mi)

    child = tools.create_asset("MI_ParametersChild", ROOT, unreal.MaterialInstanceConstant,
                               unreal.MaterialInstanceConstantFactoryNew())
    mel.set_material_instance_parent(child, mi)
    mel.set_material_instance_scalar_parameter_value(child, "Metallic", 0.0)
    mel.update_material_instance(child)
    eal.save_loaded_asset(child)


def math():
    m = material("M_Math")
    a = c3(m, 0.2, 0.4, 0.6)
    b = c(m, 0.5)
    add = node(m, unreal.MaterialExpressionAdd)
    link(a, "", add, "A")
    link(b, "", add, "B")
    mul = node(m, unreal.MaterialExpressionMultiply, const_b=2.0)
    link(add, "", mul, "A")
    sub = node(m, unreal.MaterialExpressionSubtract, const_b=0.25)
    link(mul, "", sub, "A")
    div = node(m, unreal.MaterialExpressionDivide, const_b=3.0)
    link(sub, "", div, "A")
    one_minus = node(m, unreal.MaterialExpressionOneMinus)
    link(div, "", one_minus, "")
    power = node(m, unreal.MaterialExpressionPower, const_exponent=2.2)
    link(one_minus, "", power, "Base")
    lerp = node(m, unreal.MaterialExpressionLinearInterpolate, const_alpha=0.3)
    link(power, "", lerp, "A")
    link(c3(m, 1.0, 0.0, 0.0), "", lerp, "B")
    clamp = node(m, unreal.MaterialExpressionClamp, min_default=0.1, max_default=0.9)
    link(lerp, "", clamp, "")
    pin(clamp, "", P.MP_BASE_COLOR)

    # Roughness: frac(abs(sin(x / period))) through a mask, a dot product and a saturate.
    sine = node(m, unreal.MaterialExpressionSine, period=4.0)
    link(c(m, 1.5), "", sine, "")
    absval = node(m, unreal.MaterialExpressionAbs)
    link(sine, "", absval, "")
    frac = node(m, unreal.MaterialExpressionFrac)
    link(absval, "", frac, "")
    mask = node(m, unreal.MaterialExpressionComponentMask, r=False, g=True, b=False, a=False)
    link(a, "", mask, "")
    append = node(m, unreal.MaterialExpressionAppendVector)
    link(frac, "", append, "A")
    link(mask, "", append, "B")
    dot = node(m, unreal.MaterialExpressionDotProduct)
    link(append, "", dot, "A")
    link(node(m, unreal.MaterialExpressionConstant2Vector, r=0.5, g=0.5), "", dot, "B")
    sat = node(m, unreal.MaterialExpressionSaturate)
    link(dot, "", sat, "")
    maxed = node(m, unreal.MaterialExpressionMax, const_b=0.05)
    link(sat, "", maxed, "A")
    pin(maxed, "", P.MP_ROUGHNESS)

    # Normal: a normalized constant, through Floor and Min to keep them in the graph.
    floor = node(m, unreal.MaterialExpressionFloor)
    link(c(m, 1.7), "", floor, "")
    minimum = node(m, unreal.MaterialExpressionMin)
    link(floor, "", minimum, "A")
    link(c(m, 0.0), "", minimum, "B")
    metal = node(m, unreal.MaterialExpressionAdd)
    link(minimum, "", metal, "A")
    link(c(m, 0.0), "", metal, "B")
    pin(metal, "", P.MP_METALLIC)
    normal = node(m, unreal.MaterialExpressionNormalize)
    link(c3(m, 0.1, 0.0, 1.0), "", normal, "")
    pin(normal, "", P.MP_NORMAL)
    finish(m)


def textured(folder):
    albedo = import_texture(folder, "T_Checker", lambda x, y: (220, 40, 40, 255) if (x + y) % 2 else (40, 40, 220, 255))
    normal = import_texture(folder, "T_Normal", lambda x, y: (128 + 4 * x, 128 - 4 * y, 255, 255), srgb=False,
                            compression=unreal.TextureCompressionSettings.TC_NORMALMAP)
    masks = import_texture(folder, "T_Masks", lambda x, y: (32 * x, 32 * y, 0, 128 + 16 * x), srgb=False,
                           compression=unreal.TextureCompressionSettings.TC_MASKS)
    eal.save_directory(TEXTURES)

    m = material("M_Textured")
    uv = node(m, unreal.MaterialExpressionTextureCoordinate, coordinate_index=0, u_tiling=2.0, v_tiling=3.0)
    panner = node(m, unreal.MaterialExpressionPanner, speed_x=0.1, speed_y=0.0)
    link(uv, "", panner, "Coordinate")
    base = node(m, unreal.MaterialExpressionTextureSample, texture=albedo)
    link(panner, "", base, "UVs")
    pin(base, "RGB", P.MP_BASE_COLOR)
    nmap = node(m, unreal.MaterialExpressionTextureSample, texture=normal,
                sampler_type=unreal.MaterialSamplerType.SAMPLERTYPE_NORMAL)
    link(uv, "", nmap, "UVs")
    pin(nmap, "RGB", P.MP_NORMAL)
    mask = node(m, unreal.MaterialExpressionTextureSampleParameter2D, parameter_name="Masks", texture=masks,
                sampler_type=unreal.MaterialSamplerType.SAMPLERTYPE_MASKS)
    pin(mask, "R", P.MP_ROUGHNESS)
    pin(mask, "G", P.MP_METALLIC)
    finish(m)


def functions():
    f = tools.create_asset("MF_Tint", ROOT, unreal.MaterialFunction, unreal.MaterialFunctionFactoryNew())
    color = fnode(f, unreal.MaterialExpressionFunctionInput, input_name="Color",
                  input_type=unreal.FunctionInputType.FUNCTION_INPUT_VECTOR3, sort_priority=0)
    scale = fnode(f, unreal.MaterialExpressionFunctionInput, input_name="Scale",
                  input_type=unreal.FunctionInputType.FUNCTION_INPUT_SCALAR, sort_priority=1,
                  use_preview_value_as_default=True)
    link(fnode(f, unreal.MaterialExpressionConstant, r=0.5), "", scale, "Preview")
    mul = fnode(f, unreal.MaterialExpressionMultiply)
    link(color, "", mul, "A")
    link(scale, "", mul, "B")
    out = fnode(f, unreal.MaterialExpressionFunctionOutput, output_name="Result", sort_priority=0)
    link(mul, "", out, "")
    lum = fnode(f, unreal.MaterialExpressionDesaturation)
    link(mul, "", lum, "")
    out2 = fnode(f, unreal.MaterialExpressionFunctionOutput, output_name="Gray", sort_priority=1)
    link(lum, "", out2, "")
    mel.update_material_function(f)
    eal.save_loaded_asset(f)

    m = material("M_Function")
    call = node(m, unreal.MaterialExpressionMaterialFunctionCall)
    call.set_editor_property("material_function", f)
    link(c3(m, 0.9, 0.6, 0.3), "", call, "Color")
    pin(call, "Result", P.MP_BASE_COLOR)

    breakout = node(m, unreal.MaterialExpressionMaterialFunctionCall)
    breakout.set_editor_property(
        "material_function",
        unreal.load_asset("/Engine/Functions/Engine_MaterialFunctions02/Utility/BreakOutFloat3Components"),
    )
    link(c3(m, 0.3, 0.6, 0.9), "", breakout, "Float3")
    pin(breakout, "G", P.MP_ROUGHNESS)
    finish(m)


def blending(folder):
    m = material("M_Masked", blend_mode=unreal.BlendMode.BLEND_MASKED, opacity_mask_clip_value=0.4)
    pin(c3(m, 0.2, 0.8, 0.2), "", P.MP_BASE_COLOR)
    uv = node(m, unreal.MaterialExpressionTextureCoordinate)
    mask = node(m, unreal.MaterialExpressionComponentMask, r=True, g=False, b=False, a=False)
    link(uv, "", mask, "")
    pin(mask, "", P.MP_OPACITY_MASK)
    finish(m)

    m = material("M_Translucent", blend_mode=unreal.BlendMode.BLEND_TRANSLUCENT)
    pin(c3(m, 0.9, 0.9, 1.0), "", P.MP_BASE_COLOR)
    pin(c(m, 0.35), "", P.MP_OPACITY)
    pin(c(m, 1.33), "", P.MP_REFRACTION)
    finish(m)


def models():
    m = material("M_Unlit", shading_model=unreal.MaterialShadingModel.MSM_UNLIT)
    glow = node(m, unreal.MaterialExpressionVectorParameter, parameter_name="Glow",
                default_value=unreal.LinearColor(4.0, 2.0, 0.5, 1.0))
    t = node(m, unreal.MaterialExpressionTime)
    sine = node(m, unreal.MaterialExpressionSine, period=2.0)
    link(t, "", sine, "")
    mul = node(m, unreal.MaterialExpressionMultiply)
    link(glow, "", mul, "A")
    link(sine, "", mul, "B")
    pin(mul, "", P.MP_EMISSIVE_COLOR)
    finish(m)

    # Python cannot name the custom data pins (they are hidden enum values); MakeMaterialAttributes reaches them.
    m = material("M_ClearCoat", shading_model=unreal.MaterialShadingModel.MSM_CLEAR_COAT,
                 use_material_attributes=True)
    make = node(m, unreal.MaterialExpressionMakeMaterialAttributes)
    link(c3(m, 0.6, 0.0, 0.0), "", make, "BaseColor")
    link(c(m, 1.0), "", make, "Metallic")
    link(c(m, 0.5), "", make, "Roughness")
    link(c(m, 1.0), "", make, "ClearCoat")
    link(c(m, 0.05), "", make, "ClearCoatRoughness")
    pin(make, "", P.MP_MATERIAL_ATTRIBUTES)
    finish(m)

    m = material("M_Cloth", shading_model=unreal.MaterialShadingModel.MSM_CLOTH, use_material_attributes=True)
    make = node(m, unreal.MaterialExpressionMakeMaterialAttributes)
    link(c3(m, 0.2, 0.2, 0.6), "", make, "BaseColor")
    link(c(m, 0.8), "", make, "Roughness")
    link(c3(m, 0.9, 0.9, 1.0), "", make, "SubsurfaceColor")
    link(c(m, 1.0), "", make, "ClearCoat")
    pin(make, "", P.MP_MATERIAL_ATTRIBUTES)
    finish(m)

    m = material("M_Subsurface", shading_model=unreal.MaterialShadingModel.MSM_SUBSURFACE)
    pin(c3(m, 0.8, 0.6, 0.5), "", P.MP_BASE_COLOR)
    pin(c(m, 0.5), "", P.MP_ROUGHNESS)
    pin(c3(m, 0.9, 0.2, 0.1), "", P.MP_SUBSURFACE_COLOR)
    pin(c(m, 0.7), "", P.MP_OPACITY)
    finish(m)


def geometry():
    m = material("M_Geometry")
    world = node(m, unreal.MaterialExpressionWorldPosition)
    scaled = node(m, unreal.MaterialExpressionMultiply, const_b=0.01)
    link(world, "", scaled, "A")
    frac = node(m, unreal.MaterialExpressionFrac)
    link(scaled, "", frac, "")
    reroute = node(m, unreal.MaterialExpressionReroute)
    link(frac, "", reroute, "")
    vcolor = node(m, unreal.MaterialExpressionVertexColor)
    lerp = node(m, unreal.MaterialExpressionLinearInterpolate)
    link(reroute, "", lerp, "A")
    link(vcolor, "", lerp, "B")
    link(vcolor, "A", lerp, "Alpha")
    pin(lerp, "", P.MP_BASE_COLOR)

    fresnel = node(m, unreal.MaterialExpressionFresnel, exponent=3.0, base_reflect_fraction=0.1)
    branch = node(m, unreal.MaterialExpressionIf)
    link(fresnel, "", branch, "A")
    link(c(m, 0.5), "", branch, "B")
    link(c(m, 0.9), "", branch, "A > B")
    link(c(m, 0.4), "", branch, "A == B")
    link(c(m, 0.2), "", branch, "A < B")
    quality = node(m, unreal.MaterialExpressionQualitySwitch)
    link(branch, "", quality, "Default")
    link(c(m, 0.5), "", quality, "Low")
    pin(quality, "", P.MP_ROUGHNESS)

    normal = node(m, unreal.MaterialExpressionVertexNormalWS)
    camera = node(m, unreal.MaterialExpressionCameraVectorWS)
    dot = node(m, unreal.MaterialExpressionDotProduct)
    link(normal, "", dot, "A")
    link(camera, "", dot, "B")
    custom = node(m, unreal.MaterialExpressionCustom, code="return 0.25;")
    mul = node(m, unreal.MaterialExpressionMultiply)
    link(dot, "", mul, "A")
    link(custom, "", mul, "B")
    pin(mul, "", P.MP_METALLIC)
    finish(m)


def attributes():
    m = material("M_Attributes", use_material_attributes=True)
    make = node(m, unreal.MaterialExpressionMakeMaterialAttributes)
    link(c3(m, 0.1, 0.3, 0.8), "", make, "BaseColor")
    link(c(m, 0.6), "", make, "Roughness")
    link(c(m, 1.0), "", make, "Metallic")
    pin(make, "", P.MP_MATERIAL_ATTRIBUTES)
    finish(m)


def main():
    if eal.does_directory_exist(ROOT):
        eal.delete_directory(ROOT)
    with tempfile.TemporaryDirectory() as folder:
        constants()
        parameters()
        math()
        textured(folder)
        functions()
        blending(folder)
        models()
        geometry()
        attributes()
    eal.save_directory(ROOT)
    unreal.log("Shadewire fixtures built")


main()
