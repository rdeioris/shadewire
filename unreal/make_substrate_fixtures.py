"""Builds the Substrate fixture materials in the test project, under /Game/Substrate, from scratch.

Substrate is a project setting; run the editor with it on:
    UnrealEditor-Cmd unreal/ShadewireTest/ShadewireTest.uproject -run=pythonscript
        -script=<abs>/unreal/make_substrate_fixtures.py "-ini:Engine:[/Script/Engine.RendererSettings]:r.Substrate=True"
then dump them with the same setting:
    UnrealEditor-Cmd unreal/ShadewireTest/ShadewireTest.uproject -run=ShadewireExport -folder=/Game/Substrate
        -out=<abs>/unreal/fixtures/graphs "-ini:Engine:[/Script/Engine.RendererSettings]:r.Substrate=True"
"""

import unreal

ROOT = "/Game/Substrate"

mel = unreal.MaterialEditingLibrary
eal = unreal.EditorAssetLibrary
tools = unreal.AssetToolsHelpers.get_asset_tools()
P = unreal.MaterialProperty


def material(name, **settings):
    m = tools.create_asset(name, ROOT, unreal.Material, unreal.MaterialFactoryNew())
    for key, value in settings.items():
        m.set_editor_property(key, value)
    return m


def node(m, cls, **props):
    e = mel.create_material_expression(m, cls, 0, 0)
    for key, value in props.items():
        e.set_editor_property(key, value)
    return e


def fnode(f, cls, **props):
    e = mel.create_material_expression_in_function(f, cls, 0, 0)
    for key, value in props.items():
        e.set_editor_property(key, value)
    return e


def link(a, out, b, inp):
    if not mel.connect_material_expressions(a, out, b, inp):
        raise RuntimeError(f"could not link {a.get_name()}.{out!r} to {b.get_name()}.{inp!r}")


def front(m, e, out=""):
    if not mel.connect_material_property(e, out, P.MP_FRONT_MATERIAL):
        raise RuntimeError(f"could not link {e.get_name()} to the Front Material")


def c(m, value):
    return node(m, unreal.MaterialExpressionConstant, r=value)


def c3(m, r, g, b):
    return node(m, unreal.MaterialExpressionConstant3Vector, constant=unreal.LinearColor(r, g, b, 1.0))


def slab(m, albedo=None, f0=None, roughness=None):
    """A Slab BSDF with the inputs given as (r, g, b) or numbers; the others left at Unreal's defaults."""
    s = node(m, unreal.MaterialExpressionSubstrateSlabBSDF)
    if albedo is not None:
        link(c3(m, *albedo), "", s, "Diffuse Albedo")
    if f0 is not None:
        link(c3(m, *f0), "", s, "F0")
    if roughness is not None:
        link(c(m, roughness), "", s, "Roughness")
    return s


def finish(m):
    mel.recompile_material(m)
    eal.save_loaded_asset(m)


def slabs():
    m = material("S_Slab")
    s = slab(m, (0.6, 0.3, 0.1), (0.04, 0.04, 0.04), 0.35)
    link(c3(m, 1.0, 0.9, 0.8), "", s, "F90")
    link(c(m, 0.3), "", s, "Anisotropy")
    link(c3(m, 0.0, 0.0, 0.5), "", s, "Emissive Color")
    front(m, s)
    finish(m)

    m = material("S_SlabSSS")
    s = slab(m, (0.8, 0.5, 0.4), None, 0.5)
    link(c3(m, 1.0, 0.4, 0.2), "", s, "SSS MFP")
    link(c(m, 0.5), "", s, "SSS MFP Scale")
    link(c(m, 0.2), "", s, "SSS Phase Anisotropy")
    front(m, s)
    finish(m)

    m = material("S_Haze")
    s = slab(m, (0.2, 0.2, 0.25), (0.05, 0.05, 0.05))
    base = c(m, 0.2)
    haze = node(m, unreal.MaterialExpressionSubstrateHazinessToSecondaryRoughness)
    link(base, "", haze, "BaseRoughness")
    link(c(m, 0.6), "", haze, "Haziness")
    link(base, "", s, "Roughness")
    link(haze, "Second Roughness", s, "Second Roughness")
    link(haze, "Second Roughness Weight", s, "Second Roughness Weight")
    link(c(m, 0.5), "", s, "Fuzz Amount")
    link(c3(m, 0.9, 0.9, 1.0), "", s, "Fuzz Color")
    link(c(m, 0.7), "", s, "Fuzz Roughness")
    link(c(m, 1.0), "", s, "Glint Density")
    front(m, s)
    finish(m)

    m = material("S_Metalness")
    s = slab(m, roughness=0.4)
    metal = node(m, unreal.MaterialExpressionSubstrateMetalnessToDiffuseAlbedoF0)
    link(c3(m, 0.9, 0.6, 0.2), "", metal, "BaseColor")
    link(node(m, unreal.MaterialExpressionScalarParameter, parameter_name="Metal", default_value=1.0), "", metal, "Metallic")
    link(metal, "DiffuseAlbedo", s, "Diffuse Albedo")
    link(metal, "F0", s, "F0")
    mfp = node(m, unreal.MaterialExpressionSubstrateTransmittanceToMFP)
    link(c3(m, 0.8, 0.3, 0.2), "", mfp, "TransmittanceColor")
    link(mfp, "MFP", s, "SSS MFP")
    front(m, s)
    finish(m)

    m = material("S_ThinFilm")
    s = slab(m, (0.1, 0.1, 0.1), None, 0.2)
    film = node(m, unreal.MaterialExpressionSubstrateThinFilm)
    link(c3(m, 0.9, 0.9, 0.9), "", film, "F0")
    link(film, "Specular Color", s, "F0")
    link(film, "Edge Specular Color", s, "F90")
    front(m, s)
    finish(m)


def operators():
    m = material("S_Horizontal")
    mix = node(m, unreal.MaterialExpressionSubstrateHorizontalMixing)
    link(slab(m, (0.8, 0.1, 0.1), None, 0.6), "", mix, "Background")
    link(slab(m, (0.0, 0.0, 0.0), (0.95, 0.64, 0.54), 0.2), "", mix, "Foreground")
    link(node(m, unreal.MaterialExpressionScalarParameter, parameter_name="Blend", default_value=0.25), "", mix, "Mix")
    front(m, mix)
    finish(m)

    m = material("S_Vertical")
    layer = node(m, unreal.MaterialExpressionSubstrateVerticalLayering)
    link(slab(m, (0.0, 0.0, 0.0), (0.04, 0.04, 0.04), 0.05), "", layer, "Top")
    link(slab(m, (0.0, 0.0, 0.0), (1.0, 0.78, 0.34), 0.3), "", layer, "Bottom")
    front(m, layer)
    finish(m)

    m = material("S_AddWeight")
    weight = node(m, unreal.MaterialExpressionSubstrateWeight)
    link(slab(m, (0.3, 0.6, 0.3), None, 0.5), "", weight, "A")
    link(c(m, 0.5), "", weight, "Weight")
    glow = node(m, unreal.MaterialExpressionSubstrateUnlitBSDF)
    link(c3(m, 2.0, 1.0, 0.2), "", glow, "EmissiveColor")
    add = node(m, unreal.MaterialExpressionSubstrateAdd)
    link(weight, "", add, "A")
    link(glow, "", add, "B")
    front(m, add)
    finish(m)

    m = material("S_Select")
    pick = node(m, unreal.MaterialExpressionSubstrateSelect)
    link(slab(m, (0.9, 0.9, 0.9), None, 0.3), "", pick, "A")
    link(slab(m, (0.1, 0.1, 0.1), None, 0.8), "", pick, "B")
    uv = node(m, unreal.MaterialExpressionTextureCoordinate)
    u = node(m, unreal.MaterialExpressionComponentMask, r=True, g=False, b=False, a=False)
    link(uv, "", u, "")
    link(u, "", pick, "SelectValue")
    front(m, pick)
    finish(m)

    m = material("S_ClearCoat")
    coat = node(m, unreal.MaterialExpressionSubstrateSimpleClearCoatBSDF)
    link(c3(m, 0.5, 0.0, 0.0), "", coat, "Diffuse Albedo")
    link(c(m, 0.6), "", coat, "Roughness")
    link(c(m, 0.8), "", coat, "Clear Coat Coverage")
    link(c(m, 0.02), "", coat, "Clear Coat Roughness")
    front(m, coat)
    finish(m)


def legacy():
    m = material("S_ShadingModels")
    models = node(m, unreal.MaterialExpressionSubstrateShadingModels)
    link(c3(m, 0.7, 0.7, 0.75), "", models, "BaseColor")
    link(c(m, 1.0), "", models, "Metallic")
    link(c(m, 0.25), "", models, "Roughness")
    front(m, models)
    finish(m)

    m = material("S_ShadingModelsMixed")
    models = node(m, unreal.MaterialExpressionSubstrateShadingModels)
    link(c3(m, 0.2, 0.4, 0.8), "", models, "BaseColor")
    link(c(m, 0.5), "", models, "Roughness")
    mix = node(m, unreal.MaterialExpressionSubstrateHorizontalMixing)
    link(models, "", mix, "Background")
    link(slab(m, (0.9, 0.9, 0.2), None, 0.4), "", mix, "Foreground")
    link(c(m, 0.5), "", mix, "Mix")
    front(m, mix)
    finish(m)


def structure():
    f = tools.create_asset("MF_Slab", ROOT, unreal.MaterialFunction, unreal.MaterialFunctionFactoryNew())
    tint = fnode(f, unreal.MaterialExpressionFunctionInput, input_name="Tint",
                 input_type=unreal.FunctionInputType.FUNCTION_INPUT_VECTOR3, sort_priority=0)
    s = fnode(f, unreal.MaterialExpressionSubstrateSlabBSDF)
    link(tint, "", s, "Diffuse Albedo")
    link(fnode(f, unreal.MaterialExpressionConstant, r=0.45), "", s, "Roughness")
    out = fnode(f, unreal.MaterialExpressionFunctionOutput, output_name="Slab", sort_priority=0)
    link(s, "", out, "")
    mel.update_material_function(f)
    eal.save_loaded_asset(f)

    m = material("S_Function")
    call = node(m, unreal.MaterialExpressionMaterialFunctionCall)
    call.set_editor_property("material_function", f)
    link(c3(m, 0.2, 0.7, 0.3), "", call, "Tint")
    switch = node(m, unreal.MaterialExpressionStaticSwitchParameter, parameter_name="UseFunction", default_value=True)
    link(call, "Slab", switch, "True")
    link(slab(m, (1.0, 0.0, 1.0), None, 0.5), "", switch, "False")
    front(m, switch)
    finish(m)

    m = material("S_Masked", blend_mode=unreal.BlendMode.BLEND_MASKED, opacity_mask_clip_value=0.5)
    front(m, slab(m, (0.3, 0.5, 0.2), None, 0.6))
    uv = node(m, unreal.MaterialExpressionTextureCoordinate)
    v = node(m, unreal.MaterialExpressionComponentMask, r=False, g=True, b=False, a=False)
    link(uv, "", v, "")
    if not mel.connect_material_property(v, "", P.MP_OPACITY_MASK):
        raise RuntimeError("could not link the opacity mask")
    finish(m)


def main():
    if unreal.SystemLibrary.get_console_variable_int_value("r.Substrate") != 1:
        raise RuntimeError("run the editor with r.Substrate=True (see this script's docstring)")
    if eal.does_directory_exist(ROOT):
        eal.delete_directory(ROOT)
    slabs()
    operators()
    legacy()
    structure()
    eal.save_directory(ROOT)


main()
