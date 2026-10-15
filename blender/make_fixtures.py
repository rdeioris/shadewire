"""Writes the graph fixtures: ten materials built here, dumped by the add-on's graph module, one JSON file each in
blender/fixtures/graphs. The Rust front end's tests read those files, and test_graph.py checks they are current.

blender -b --python-exit-code 1 -P blender/make_fixtures.py

Rerunning in the same Blender changes nothing; another Blender changes the `producer` line and whatever its node
defaults and enums differ in.
"""

import importlib.util
import os
import sys

import bpy

here = os.path.dirname(os.path.abspath(__file__))
folder = os.path.join(here, "fixtures", "graphs")


def load_addon():
    addon_folder = os.path.join(here, "shadewire")
    spec = importlib.util.spec_from_file_location("shadewire_addon", os.path.join(addon_folder, "__init__.py"),
                                                  submodule_search_locations=[addon_folder])
    addon = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = addon
    spec.loader.exec_module(addon)
    addon.register()
    return addon


def new_material(name):
    material = bpy.data.materials.new(name)
    material.use_nodes = True
    return material, material.node_tree


def add(tree, idname, col, row, name=None, label=None, **properties):
    """A node on a grid, with optional name, label and properties."""
    node = tree.nodes.new(idname)
    node.location = (col * 260, -row * 230)
    if name:
        node.name = name
    if label:
        node.label = label
    for key, value in properties.items():
        setattr(node, key, value)
    return node


def sock(sockets, identifier):
    """A socket by identifier: the names of some nodes' sockets depend on a mode."""
    return next(s for s in sockets if s.identifier == identifier)


def link(tree, source, target):
    tree.links.new(source, target)


def principled_of(tree):
    return next(n for n in tree.nodes if n.bl_idname == "ShaderNodeBsdfPrincipled")


def output_of(tree):
    return next(n for n in tree.nodes if n.bl_idname == "ShaderNodeOutputMaterial")


def value_node(tree, value, col, row, name):
    node = add(tree, "ShaderNodeValue", col, row, name)
    node.outputs[0].default_value = value
    return node


def rgb_node(tree, color, col, row, name):
    node = add(tree, "ShaderNodeRGB", col, row, name)
    node.outputs[0].default_value = color
    return node


def generated_image(name, colorspace):
    image = bpy.data.images.new(name, 4, 4)
    image.colorspace_settings.name = colorspace
    return image


def principled_defaults():
    material, _ = new_material("principled_defaults")
    return material


def principled_everything():
    material, t = new_material("principled_everything")
    p = principled_of(t)
    p.distribution = "GGX"
    p.inputs["Thin Wall"].default_value = True
    palette = [(0.8, 0.2, 0.1, 1.0), (0.1, 0.6, 0.3, 1.0), (0.2, 0.3, 0.9, 1.0), (0.9, 0.8, 0.1, 1.0)]
    row = 0
    for i, socket in enumerate(p.inputs):
        if not socket.enabled or socket.is_unavailable or socket.name == "Thin Wall":
            continue
        name = socket.name
        if socket.type == "VALUE":
            value = (i % 7 + 1) / 8
            source = value_node(t, 1.0 + value if "IOR" in name else value, -2, row, f"{name} Value")
        elif socket.type == "RGBA":
            source = rgb_node(t, palette[i % len(palette)], -2, row, f"{name} Color")
        elif name in ("Normal", "Coat Normal"):
            source = add(t, "ShaderNodeNormalMap", -2, row, f"{name} Map")
            source.inputs["Strength"].default_value = 0.5
        elif name == "Tangent":
            source = add(t, "ShaderNodeTangent", -2, row, "Tangent", direction_type="UV_MAP")
        else:
            source = add(t, "ShaderNodeCombineXYZ", -2, row, f"{name} Vector")
            source.inputs[0].default_value = 0.5
            source.inputs[1].default_value = 0.25
            source.inputs[2].default_value = 0.125
        link(t, source.outputs[0], socket)
        row += 1
    return material


def textured():
    material, t = new_material("textured")
    p = principled_of(t)
    uv = add(t, "ShaderNodeTexCoord", -4, 0, "Coordinates")
    mapping = add(t, "ShaderNodeMapping", -3, 0, "Tiling")
    mapping.inputs["Scale"].default_value = (2.0, 2.0, 1.0)
    link(t, uv.outputs["UV"], mapping.inputs["Vector"])
    albedo = add(t, "ShaderNodeTexImage", -2, 0, "Albedo")
    albedo.image = generated_image("albedo", "Non-Color")
    albedo.extension = "EXTEND"
    link(t, mapping.outputs["Vector"], albedo.inputs["Vector"])
    link(t, albedo.outputs["Color"], p.inputs["Base Color"])
    normal = add(t, "ShaderNodeTexImage", -3, 2, "Normal Image")
    normal.image = generated_image("normal", "Non-Color")
    normal.interpolation = "Closest"
    normal_map = add(t, "ShaderNodeNormalMap", -2, 2, "Normal Map")
    link(t, normal.outputs["Color"], normal_map.inputs["Color"])
    uv_map = add(t, "ShaderNodeUVMap", -4, 4, "UV Map", uv_map="UVMap")
    rough = add(t, "ShaderNodeTexImage", -2, 4, "Roughness Image")
    rough.image = generated_image("roughness", "sRGB")
    link(t, uv_map.outputs["UV"], rough.inputs["Vector"])
    link(t, rough.outputs["Color"], p.inputs["Roughness"])

    # Bump over the tangent-space normal map: a height image in Box projection, Clip extension, Cubic interpolation.
    height = add(t, "ShaderNodeTexImage", -2, 6, "Height Image", projection="BOX", extension="CLIP",
                 interpolation="Cubic")
    height.image = generated_image("height", "Non-Color")
    link(t, uv.outputs["Object"], height.inputs["Vector"])
    bump = add(t, "ShaderNodeBump", -1, 4, "Bump")
    bump.inputs["Distance"].default_value = 0.2
    link(t, height.outputs["Color"], bump.inputs["Height"])
    link(t, normal_map.outputs["Normal"], bump.inputs["Normal"])
    link(t, bump.outputs["Normal"], p.inputs["Normal"])

    # Normal maps in Object and World space; the World one turned by a Mapping of the Vector type into the tangent.
    object_map = add(t, "ShaderNodeNormalMap", -2, 8, "Normal Map Object", space="OBJECT")
    link(t, normal.outputs["Color"], object_map.inputs["Color"])
    link(t, object_map.outputs["Normal"], p.inputs["Coat Normal"])
    world_map = add(t, "ShaderNodeNormalMap", -2, 10, "Normal Map World", space="WORLD")
    link(t, normal.outputs["Color"], world_map.inputs["Color"])
    turn = add(t, "ShaderNodeMapping", -1, 10, "Turn Tangent", vector_type="VECTOR")
    turn.inputs["Rotation"].default_value = (0.0, 0.0, 0.5)
    link(t, world_map.outputs["Normal"], turn.inputs["Vector"])
    link(t, turn.outputs["Vector"], p.inputs["Tangent"])

    # A Mapping of the Texture type into an image in Mirror extension.
    texture_space = add(t, "ShaderNodeMapping", -3, 12, "Texture Space", vector_type="TEXTURE")
    texture_space.inputs["Location"].default_value = (0.25, 0.5, 0.0)
    link(t, uv.outputs["UV"], texture_space.inputs["Vector"])
    mirror = add(t, "ShaderNodeTexImage", -2, 12, "Mirror Image", extension="MIRROR")
    mirror.image = generated_image("emission", "sRGB")
    link(t, texture_space.outputs["Vector"], mirror.inputs["Vector"])
    link(t, mirror.outputs["Color"], p.inputs["Emission Color"])
    return material


def math():
    material, t = new_material("math")
    t.nodes.remove(principled_of(t))
    emission = add(t, "ShaderNodeEmission", 8, 0, "Emission")
    link(t, emission.outputs[0], output_of(t).inputs["Surface"])

    # Every Math operation once, in a chain: the first node reads a Value, each later one the node before it.
    operations = [item.identifier for item in bpy.types.ShaderNodeMath.bl_rna.properties["operation"].enum_items]
    source = value_node(t, 0.75, -2, 0, "Start")
    previous = source.outputs[0]
    for i, operation in enumerate(operations):
        node = add(t, "ShaderNodeMath", -1 + i % 10, 1 + i // 10, f"Math {operation}", operation=operation)
        node.inputs[1].default_value = 0.5
        link(t, previous, node.inputs[0])
        previous = node.outputs[0]
    chain_end = previous

    # Vectors: Combine XYZ, a chain of the Vector Math operations that give a vector (Scale scaled by the Length of the
    # vector so far), Mix in Non-Uniform mode, Separate XYZ. Dot Product and Distance, which give a value, are added
    # into the Blue of the Combine Color below.
    col = 0
    row = 8
    combine = add(t, "ShaderNodeCombineXYZ", col, row, "Combine XYZ")
    link(t, chain_end, combine.inputs["X"])
    combine.inputs["Y"].default_value = 0.25
    combine.inputs["Z"].default_value = 0.5
    vector = combine.outputs[0]
    for operation in ("ADD", "MULTIPLY", "CROSS_PRODUCT", "NORMALIZE", "SCALE", "REFLECT", "PROJECT", "FACEFORWARD",
                      "WRAP", "SNAP", "SINE", "FLOOR", "FRACTION", "MODULO", "ABSOLUTE", "MINIMUM", "MAXIMUM"):
        col += 1
        node = add(t, "ShaderNodeVectorMath", col % 10, row + 3 * (col // 10), f"Vector {operation}",
                   operation=operation)
        node.inputs[1].default_value = (0.5, 0.25, 0.75)
        node.inputs[2].default_value = (-0.5, -0.25, 0.0)
        link(t, vector, node.inputs[0])
        if operation == "SCALE":
            length = add(t, "ShaderNodeVectorMath", col % 10, row + 1 + 3 * (col // 10), "Vector LENGTH",
                         operation="LENGTH")
            link(t, vector, length.inputs[0])
            link(t, length.outputs["Value"], node.inputs["Scale"])
        vector = node.outputs["Vector"]
    dot = add(t, "ShaderNodeVectorMath", 0, row + 2, "Vector DOT_PRODUCT", operation="DOT_PRODUCT")
    link(t, combine.outputs[0], dot.inputs[0])
    link(t, vector, dot.inputs[1])
    distance = add(t, "ShaderNodeVectorMath", 1, row + 2, "Vector DISTANCE", operation="DISTANCE")
    link(t, combine.outputs[0], distance.inputs[0])
    link(t, vector, distance.inputs[1])
    values = add(t, "ShaderNodeMath", 2, row + 2, "Dot Plus Distance", operation="ADD")
    link(t, dot.outputs["Value"], values.inputs[0])
    link(t, distance.outputs["Value"], values.inputs[1])
    col = 8
    row = 11
    mix_vector = add(t, "ShaderNodeMix", col, row, "Mix Vector", data_type="VECTOR", factor_mode="NON_UNIFORM")
    sock(mix_vector.inputs, "Factor_Vector").default_value = (0.2, 0.5, 0.8)
    sock(mix_vector.inputs, "B_Vector").default_value = (1.0, 0.5, 0.0)
    link(t, vector, sock(mix_vector.inputs, "A_Vector"))
    col += 1
    separate = add(t, "ShaderNodeSeparateXYZ", col, row, "Separate XYZ")
    link(t, sock(mix_vector.outputs, "Result_Vector"), separate.inputs[0])

    # Floats: Clamp, Map Range in Linear and Smooth Step, Mix in Float mode, Combine XYZ again.
    row = 13
    col = 0
    clamp = add(t, "ShaderNodeClamp", col, row, "Clamp")
    clamp.inputs["Min"].default_value = 0.1
    clamp.inputs["Max"].default_value = 0.9
    link(t, separate.outputs["X"], clamp.inputs["Value"])
    linear = add(t, "ShaderNodeMapRange", col + 1, row, "Map Range Linear", interpolation_type="LINEAR")
    linear.inputs["To Min"].default_value = 0.2
    linear.inputs["To Max"].default_value = 0.8
    link(t, clamp.outputs[0], linear.inputs["Value"])
    smooth = add(t, "ShaderNodeMapRange", col + 2, row, "Map Range Smooth", interpolation_type="SMOOTHSTEP")
    link(t, linear.outputs["Result"], smooth.inputs["Value"])
    mix_float = add(t, "ShaderNodeMix", col + 3, row, "Mix Float", data_type="FLOAT")
    sock(mix_float.inputs, "Factor_Float").default_value = 0.3
    link(t, smooth.outputs["Result"], sock(mix_float.inputs, "A_Float"))
    link(t, separate.outputs["Y"], sock(mix_float.inputs, "B_Float"))
    link(t, smooth.outputs["Result"], emission.inputs["Strength"])

    # Colors: Combine Color (RGB) -> Separate Color (HSV) -> Combine Color (HSV) -> Separate Color (RGB)
    # -> Combine Color (RGB) -> Separate Color (HSL) -> Combine Color (HSL), then Mix in Color mode (Multiply), the
    # color adjustments and one Mix in Color mode for each further blend mode.
    row = 14
    combine_rgb = add(t, "ShaderNodeCombineColor", 0, row, "Combine RGB", mode="RGB")
    link(t, sock(mix_float.outputs, "Result_Float"), sock(combine_rgb.inputs, "Red"))
    link(t, separate.outputs["Z"], sock(combine_rgb.inputs, "Green"))
    link(t, values.outputs[0], sock(combine_rgb.inputs, "Blue"))
    separate_hsv = add(t, "ShaderNodeSeparateColor", 1, row, "Separate HSV", mode="HSV")
    link(t, combine_rgb.outputs[0], separate_hsv.inputs[0])
    combine_hsv = add(t, "ShaderNodeCombineColor", 2, row, "Combine HSV", mode="HSV")
    for channel in ("Red", "Green", "Blue"):
        link(t, sock(separate_hsv.outputs, channel), sock(combine_hsv.inputs, channel))
    separate_rgb = add(t, "ShaderNodeSeparateColor", 3, row, "Separate RGB", mode="RGB")
    link(t, combine_hsv.outputs[0], separate_rgb.inputs[0])
    combine_back = add(t, "ShaderNodeCombineColor", 4, row, "Combine RGB Again", mode="RGB")
    for channel in ("Red", "Green", "Blue"):
        link(t, sock(separate_rgb.outputs, channel), sock(combine_back.inputs, channel))
    separate_hsl = add(t, "ShaderNodeSeparateColor", 5, row, "Separate HSL", mode="HSL")
    link(t, combine_back.outputs[0], separate_hsl.inputs[0])
    combine_hsl = add(t, "ShaderNodeCombineColor", 6, row, "Combine HSL", mode="HSL")
    for channel in ("Red", "Green", "Blue"):
        link(t, sock(separate_hsl.outputs, channel), sock(combine_hsl.inputs, channel))
    mix_color = add(t, "ShaderNodeMix", 7, row, "Mix Color", data_type="RGBA", blend_type="MULTIPLY")
    sock(mix_color.inputs, "Factor_Float").default_value = 0.6
    sock(mix_color.inputs, "B_Color").default_value = (0.9, 0.7, 0.5, 1.0)
    link(t, combine_hsl.outputs[0], sock(mix_color.inputs, "A_Color"))
    color = sock(mix_color.outputs, "Result_Color")
    for i, idname in enumerate(("ShaderNodeInvert", "ShaderNodeGamma", "ShaderNodeBrightContrast",
                                "ShaderNodeHueSaturation")):
        node = add(t, idname, 8 + i, row)
        link(t, color, node.inputs["Color"])
        color = node.outputs["Color"]
    blends = ("SCREEN", "OVERLAY", "ADD", "SUBTRACT", "DIFFERENCE", "DARKEN", "LIGHTEN", "DIVIDE", "SOFT_LIGHT",
              "LINEAR_LIGHT", "EXCLUSION")
    for i, blend in enumerate(blends):
        node = add(t, "ShaderNodeMix", i, row + 1, f"Mix {blend}", data_type="RGBA", blend_type=blend)
        sock(node.inputs, "Factor_Float").default_value = 0.5
        sock(node.inputs, "B_Color").default_value = (0.25 + i / 20, 0.5, 0.75 - i / 20, 1.0)
        link(t, color, sock(node.inputs, "A_Color"))
        color = sock(node.outputs, "Result_Color")
    link(t, color, emission.inputs["Color"])
    return material


def procedural():
    material, t = new_material("procedural")
    p = principled_of(t)
    coordinates = add(t, "ShaderNodeTexCoord", -4, 0, "Coordinates")
    geometry = add(t, "ShaderNodeNewGeometry", -4, 3, "Geometry")
    mask = add(t, "ShaderNodeAttribute", -4, 6, "Mask", attribute_name="mask")
    color_attribute = add(t, "ShaderNodeVertexColor", -4, 8, "Vertex Color", layer_name="Col")

    noise3 = add(t, "ShaderNodeTexNoise", -2, 0, "Noise 3D", noise_dimensions="3D")
    link(t, coordinates.outputs["Generated"], noise3.inputs["Vector"])
    link(t, noise3.outputs["Color"], p.inputs["Base Color"])
    noise4 = add(t, "ShaderNodeTexNoise", -2, 2, "Noise 4D", noise_dimensions="4D")
    link(t, coordinates.outputs["Object"], noise4.inputs["Vector"])
    link(t, mask.outputs["Fac"], noise4.inputs["W"])
    link(t, noise4.outputs["Fac"], p.inputs["Roughness"])

    voronoi = add(t, "ShaderNodeTexVoronoi", -2, 4, "Voronoi F1", feature="F1")
    link(t, geometry.outputs["Position"], voronoi.inputs["Vector"])
    link(t, voronoi.outputs["Distance"], p.inputs["Metallic"])
    smooth = add(t, "ShaderNodeTexVoronoi", -2, 6, "Voronoi Smooth", feature="SMOOTH_F1")
    link(t, geometry.outputs["Position"], smooth.inputs["Vector"])
    link(t, smooth.outputs["Color"], p.inputs["Emission Color"])

    white = add(t, "ShaderNodeTexWhiteNoise", -2, 8, "White Noise")
    link(t, geometry.outputs["Normal"], white.inputs["Vector"])
    link(t, white.outputs["Value"], p.inputs["Emission Strength"])
    checker = add(t, "ShaderNodeTexChecker", -2, 10, "Checker")
    link(t, coordinates.outputs["UV"], checker.inputs["Vector"])
    link(t, checker.outputs["Fac"], p.inputs["Alpha"])
    linear = add(t, "ShaderNodeTexGradient", -2, 12, "Gradient Linear", gradient_type="LINEAR")
    link(t, coordinates.outputs["Camera"], linear.inputs["Vector"])
    link(t, linear.outputs["Fac"], p.inputs["Coat Weight"])
    spherical = add(t, "ShaderNodeTexGradient", -2, 14, "Gradient Spherical", gradient_type="SPHERICAL")
    link(t, coordinates.outputs["Window"], spherical.inputs["Vector"])
    link(t, spherical.outputs["Fac"], p.inputs["Sheen Weight"])

    link(t, color_attribute.outputs["Color"], p.inputs["Specular Tint"])
    link(t, geometry.outputs["Normal"], p.inputs["Normal"])

    noise2 = add(t, "ShaderNodeTexNoise", -2, 16, "Noise 2D", noise_dimensions="2D")
    link(t, coordinates.outputs["UV"], noise2.inputs["Vector"])
    link(t, noise2.outputs["Fac"], p.inputs["Subsurface Weight"])
    f2 = add(t, "ShaderNodeTexVoronoi", -2, 18, "Voronoi F2", feature="F2")
    link(t, coordinates.outputs["Reflection"], f2.inputs["Vector"])
    link(t, f2.outputs["Distance"], p.inputs["Specular IOR Level"])
    edge = add(t, "ShaderNodeTexVoronoi", -2, 20, "Voronoi Edge", feature="DISTANCE_TO_EDGE")
    link(t, coordinates.outputs["Object"], edge.inputs["Vector"])
    link(t, edge.outputs["Distance"], p.inputs["Anisotropic"])
    sphere = add(t, "ShaderNodeTexVoronoi", -2, 22, "Voronoi Sphere", feature="N_SPHERE_RADIUS")
    link(t, coordinates.outputs["Generated"], sphere.inputs["Vector"])
    link(t, sphere.outputs["Radius"], p.inputs["Transmission Weight"])
    for i, (kind, socket, target) in enumerate((("QUADRATIC", "Generated", "Coat Roughness"),
                                                 ("DIAGONAL", "Object", "Sheen Roughness"),
                                                 ("RADIAL", "UV", "Thin Film Thickness"))):
        gradient = add(t, "ShaderNodeTexGradient", -2, 24 + 2 * i, f"Gradient {kind.title()}", gradient_type=kind)
        link(t, coordinates.outputs[socket], gradient.inputs["Vector"])
        link(t, gradient.outputs["Fac"], p.inputs[target])
    return material


def lobes():
    material, t = new_material("lobes")
    t.nodes.remove(principled_of(t))
    out = output_of(t)

    def lobe(idname, row, name, **properties):
        return add(t, idname, -3, row, name, **properties)

    def mix(a, b, col, row, name, fac):
        node = add(t, "ShaderNodeMixShader", col, row, name)
        node.inputs["Fac"].default_value = fac
        link(t, a.outputs[0], node.inputs[1])
        link(t, b.outputs[0], node.inputs[2])
        return node

    def plus(a, b, col, row, name):
        node = add(t, "ShaderNodeAddShader", col, row, name)
        link(t, a.outputs[0], node.inputs[0])
        link(t, b.outputs[0], node.inputs[1])
        return node

    diffuse = lobe("ShaderNodeBsdfDiffuse", 0, "Diffuse")
    glossy = lobe("ShaderNodeBsdfGlossy", 1, "Glossy")
    glass = lobe("ShaderNodeBsdfGlass", 2, "Glass")
    refraction = lobe("ShaderNodeBsdfRefraction", 3, "Refraction")
    translucent = lobe("ShaderNodeBsdfTranslucent", 4, "Translucent")
    sheen = lobe("ShaderNodeBsdfSheen", 5, "Sheen")
    subsurface = lobe("ShaderNodeSubsurfaceScattering", 6, "Subsurface")
    conductor = lobe("ShaderNodeBsdfMetallic", 7, "Metallic Conductor", fresnel_type="PHYSICAL_CONDUCTOR")
    f82 = lobe("ShaderNodeBsdfMetallic", 8, "Metallic F82", fresnel_type="F82")
    emission = lobe("ShaderNodeEmission", 9, "Emission")
    transparent = lobe("ShaderNodeBsdfTransparent", 10, "Transparent")

    m1 = mix(diffuse, glossy, -2, 0, "Mix Diffuse Glossy", 0.3)
    m2 = mix(glass, refraction, -2, 2, "Mix Glass Refraction", 0.5)
    a1 = plus(m1, m2, -1, 1, "Add Surface")
    a2 = plus(translucent, sheen, -2, 4, "Add Translucent Sheen")
    m3 = mix(a2, subsurface, -1, 5, "Mix Soft", 0.6)
    a3 = plus(conductor, f82, -2, 7, "Add Metals")
    m4 = mix(emission, transparent, -2, 9, "Mix Emission Transparent", 0.1)
    a4 = plus(a1, m3, 0, 3, "Add Top")
    a5 = plus(a3, m4, -1, 8, "Add Bottom")
    final = mix(a4, a5, 1, 5, "Mix Final", 0.5)
    link(t, final.outputs[0], out.inputs["Surface"])
    return material


def structure():
    material, t = new_material("structure")
    p = principled_of(t)
    group = bpy.data.node_groups.new("Gain", "ShaderNodeTree")
    amount = group.interface.new_socket("Amount", in_out="INPUT", socket_type="NodeSocketFloat")
    amount.default_value = 0.5
    amount.min_value, amount.max_value = 0.0, 1.0
    group.interface.new_socket("Out", in_out="OUTPUT", socket_type="NodeSocketFloat")
    inside_in = add(group, "NodeGroupInput", -1, 0)
    scale = add(group, "ShaderNodeMath", 0, 0, "Scale", operation="MULTIPLY")
    scale.inputs[1].default_value = 2.0
    inside_out = add(group, "NodeGroupOutput", 1, 0)
    link(group, inside_in.outputs["Amount"], scale.inputs[0])
    link(group, scale.outputs[0], inside_out.inputs["Out"])

    gain = add(t, "ShaderNodeGroup", -2, 0, "Gain")
    gain.node_tree = group
    link(t, gain.outputs["Out"], p.inputs["Roughness"])

    frame = add(t, "NodeFrame", -4, 2, "Frame", label="Muted")
    source = value_node(t, 0.5, -5, 3, "Source")
    muted = add(t, "ShaderNodeMath", -4, 3, "Muted Add", operation="ADD", mute=True)
    muted.inputs[1].default_value = 0.25
    muted.parent = frame
    link(t, source.outputs[0], muted.inputs[0])
    reroute = add(t, "NodeReroute", -3, 3, "Reroute")
    link(t, muted.outputs[0], reroute.inputs[0])
    link(t, reroute.outputs[0], p.inputs["Metallic"])

    labelled = rgb_node(t, (0.2, 0.4, 0.6, 1.0), -3, 5, "Tint")
    labelled.label = "Labelled"
    link(t, labelled.outputs[0], p.inputs["Base Color"])

    unused = add(t, "ShaderNodeOutputMaterial", 2, 3, "Unused Output")
    unused.is_active_output = False
    output_of(t).is_active_output = True
    return material


def unsupported():
    material, t = new_material("unsupported")
    p = principled_of(t)
    layer = add(t, "ShaderNodeLayerWeight", -5, 0, "Layer Weight")
    fresnel = add(t, "ShaderNodeFresnel", -5, 2, "Fresnel")
    path = add(t, "ShaderNodeLightPath", -5, 4, "Light Path")
    info = add(t, "ShaderNodeObjectInfo", -5, 6, "Object Info")
    blackbody = add(t, "ShaderNodeBlackbody", -5, 8, "Blackbody")

    ramp = add(t, "ShaderNodeValToRGB", -4, 2, "Color Ramp")
    ramp.color_ramp.elements[0].color = (0.1, 0.2, 0.3, 1.0)
    ramp.color_ramp.elements[1].position = 0.9
    ramp.color_ramp.elements.new(0.5).color = (0.8, 0.4, 0.1, 1.0)
    curves = add(t, "ShaderNodeRGBCurve", -3, 2, "RGB Curves")
    curves.mapping.curves[3].points.new(0.5, 0.7)
    curves.mapping.update()
    float_curve = add(t, "ShaderNodeFloatCurve", -4, 0, "Float Curve")
    float_curve.mapping.curves[0].points.new(0.5, 0.9)
    float_curve.mapping.update()

    link(t, fresnel.outputs["Fac"], ramp.inputs["Fac"])
    link(t, ramp.outputs["Color"], curves.inputs["Color"])
    link(t, curves.outputs["Color"], p.inputs["Base Color"])
    link(t, layer.outputs["Facing"], float_curve.inputs["Value"])
    link(t, float_curve.outputs[0], p.inputs["Metallic"])
    link(t, layer.outputs["Fresnel"], p.inputs["Roughness"])
    link(t, path.outputs["Is Camera Ray"], p.inputs["Alpha"])
    link(t, blackbody.outputs["Color"], p.inputs["Emission Color"])
    link(t, info.outputs["Random"], p.inputs["Emission Strength"])

    wavelength = add(t, "ShaderNodeWavelength", -5, 10, "Wavelength")
    link(t, wavelength.outputs["Color"], p.inputs["Coat Tint"])
    occlusion = add(t, "ShaderNodeAmbientOcclusion", -5, 12, "Ambient Occlusion")
    link(t, occlusion.outputs["Color"], p.inputs["Sheen Tint"])
    bevel = add(t, "ShaderNodeBevel", -5, 14, "Bevel")
    link(t, bevel.outputs["Normal"], p.inputs["Coat Normal"])
    wireframe = add(t, "ShaderNodeWireframe", -5, 16, "Wireframe")
    link(t, wireframe.outputs["Fac"], p.inputs["Coat Weight"])
    diffuse = add(t, "ShaderNodeBsdfDiffuse", -5, 18, "Diffuse")
    to_rgb = add(t, "ShaderNodeShaderToRGB", -4, 18, "Shader to RGB")
    link(t, diffuse.outputs["BSDF"], to_rgb.inputs["Shader"])
    link(t, to_rgb.outputs["Color"], p.inputs["Specular Tint"])
    return material


def displacement_volume():
    """A surface with displacement and a volume, each into its own socket of the output."""
    material, t = new_material("displacement_volume")
    material.displacement_method = "BOTH"
    out = output_of(t)
    noise = add(t, "ShaderNodeTexNoise", -3, 4, "Height Noise")
    displacement = add(t, "ShaderNodeDisplacement", -1, 4, "Displacement")
    displacement.inputs["Scale"].default_value = 0.1
    link(t, noise.outputs["Fac"], displacement.inputs["Height"])
    link(t, displacement.outputs["Displacement"], out.inputs["Displacement"])
    volume = add(t, "ShaderNodeVolumePrincipled", -1, 2, "Principled Volume")
    volume.inputs["Density"].default_value = 0.5
    link(t, volume.outputs["Volume"], out.inputs["Volume"])
    return material


def gltf_output():
    """A Principled BSDF whose sources also feed the `glTF Material Output` group as Blender's glTF importer makes
    it: four inputs (Occlusion, Thickness, Specular, Specular Color), no outputs."""
    material, t = new_material("gltf_output")
    p = principled_of(t)
    group = bpy.data.node_groups.new("glTF Material Output", "ShaderNodeTree")
    for name, socket_type in (("Occlusion", "NodeSocketFloat"), ("Thickness", "NodeSocketFloat"),
                              ("Specular", "NodeSocketFloat"), ("Specular Color", "NodeSocketColor")):
        group.interface.new_socket(name, in_out="INPUT", socket_type=socket_type)
    add(group, "NodeGroupOutput", 1, 0)
    add(group, "NodeGroupInput", -1, 0)
    settings = add(t, "ShaderNodeGroup", 0, 4, "glTF Material Output", label="glTF Material Output")
    settings.node_tree = group

    occlusion = add(t, "ShaderNodeTexImage", -4, 4, "Occlusion Image")
    occlusion.image = generated_image("occlusion", "Non-Color")
    channels = add(t, "ShaderNodeSeparateColor", -3, 4, "Separate Occlusion")
    link(t, occlusion.outputs["Color"], channels.inputs["Color"])
    link(t, sock(channels.outputs, "Red"), settings.inputs["Occlusion"])
    thickness = value_node(t, 0.25, -2, 6, "Thickness")
    link(t, thickness.outputs[0], settings.inputs["Thickness"])
    specular = value_node(t, 0.6, -2, 2, "Specular")
    link(t, specular.outputs[0], p.inputs["Specular IOR Level"])
    link(t, specular.outputs[0], settings.inputs["Specular"])
    specular_color = rgb_node(t, (1.0, 0.9, 0.8, 1.0), -2, 3, "Specular Color")
    link(t, specular_color.outputs[0], p.inputs["Specular Tint"])
    link(t, specular_color.outputs[0], settings.inputs["Specular Color"])
    return material


FIXTURES = {
    "principled_defaults": principled_defaults,
    "principled_everything": principled_everything,
    "textured": textured,
    "math": math,
    "procedural": procedural,
    "lobes": lobes,
    "structure": structure,
    "unsupported": unsupported,
    "displacement_volume": displacement_volume,
    "gltf_output": gltf_output,
}


def main():
    graph = load_addon().graph
    os.makedirs(folder, exist_ok=True)
    for name, build in FIXTURES.items():
        bpy.ops.wm.read_homefile(use_empty=True)
        text = graph.to_text(graph.dump([build()]))
        path = os.path.join(folder, f"{name}.json")
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
        print(f"wrote {path}")


if __name__ == "__main__":
    main()
