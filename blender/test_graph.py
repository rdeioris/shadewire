"""Tests of the add-on's graph module: dump shape, dump → load → dump equality, the Save and Load operators.

blender -b -P blender/test_graph.py
"""

import importlib.util
import json
import os
import sys
import tempfile

import bpy

here = os.path.dirname(os.path.abspath(__file__))
failures = []


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


def textured_material():
    """A Principled BSDF with an image, a math chain, a muted node, a reroute, a frame and a group."""
    m = bpy.data.materials.new("Wood")
    m.use_nodes = True
    m.surface_render_method = "BLENDED"
    t = m.node_tree
    principled = next(n for n in t.nodes if n.bl_idname == "ShaderNodeBsdfPrincipled")
    image = bpy.data.images.new("wood_albedo", 4, 4)
    tex = t.nodes.new("ShaderNodeTexImage")
    tex.name = "Albedo"
    tex.image = image
    tex.extension = "CLIP"
    t.links.new(tex.outputs["Color"], principled.inputs["Base Color"])
    math = t.nodes.new("ShaderNodeMath")
    math.operation = "MULTIPLY"
    math.inputs[1].default_value = 0.25
    math.mute = True
    frame = t.nodes.new("NodeFrame")
    math.parent = frame
    reroute = t.nodes.new("NodeReroute")
    t.links.new(math.outputs[0], reroute.inputs[0])
    t.links.new(reroute.outputs[0], principled.inputs["Roughness"])
    group = bpy.data.node_groups.new("Gain", "ShaderNodeTree")
    group.interface.new_socket("Amount", in_out="INPUT", socket_type="NodeSocketFloat").default_value = 0.5
    group.interface.new_socket("Out", in_out="OUTPUT", socket_type="NodeSocketFloat")
    gi = group.nodes.new("NodeGroupInput")
    go = group.nodes.new("NodeGroupOutput")
    group.links.new(gi.outputs[0], go.inputs[0])
    gn = t.nodes.new("ShaderNodeGroup")
    gn.node_tree = group
    t.links.new(gn.outputs[0], principled.inputs["Metallic"])
    ramp = t.nodes.new("ShaderNodeValToRGB")
    ramp.color_ramp.elements[0].color = (0.1, 0.2, 0.3, 1.0)
    return m


def test_dump_shape(graph):
    doc = graph.dump([textured_material()])
    check(doc["format"] == "shadewire-blender-graph" and doc["version"] == 1, "format header")
    check(doc["producer"].startswith("Blender "), "producer")
    [material] = doc["materials"]
    check(material["name"] == "Wood" and material["output"] == "Material Output", "material name and output")
    check(material["settings"]["surface_render_method"] == "BLENDED", "settings carry the render method")
    by_name = {n["name"]: n for n in material["nodes"]}
    math = by_name["Math"]
    check(math["type"] == "ShaderNodeMath" and math["properties"] == {"operation": "MULTIPLY", "use_clamp": False},
          f"math properties: {math['properties']}")
    check([s["identifier"] for s in math["inputs"]] == ["Value", "Value_001", "Value_002"], "socket identifiers")
    check(math["inputs"][1]["value"] == 0.25 and math["mute"] is True and math["parent"] == "Frame",
          "math value, mute, parent")
    check(math["internal_links"] == [["Value", "Value"]], f"internal links: {math['internal_links']}")
    tex = by_name["Albedo"]
    check(tex["properties"]["image"] == "wood_albedo" and tex["properties"]["extension"] == "CLIP", "image node props")
    check("texture_mapping" not in tex["properties"], "read-only pointers are not dumped")
    check(by_name["Principled BSDF"]["inputs"][0]["value"] == [0.8, 0.8, 0.8, 1.0]
          or len(by_name["Principled BSDF"]["inputs"][0]["value"]) == 4, "RGBA value is four floats")
    check("value" not in by_name["Principled BSDF"]["outputs"][0], "shader sockets have no value")
    check({"from": ["Albedo", "Color"], "to": ["Principled BSDF", "Base Color"], "muted": False, "valid": True}
          in material["links"], "link by identifiers")
    check(by_name["Reroute"]["type"] == "NodeReroute" and by_name["Frame"]["type"] == "NodeFrame", "reroute and frame")
    ramp = by_name["Color Ramp"]["properties"]["color_ramp"]
    check(ramp["interpolation"] == "LINEAR" and len(ramp["elements"]) == 2
          and ramp["elements"][0]["color"][:3] == [pytest_round(c) for c in (0.1, 0.2, 0.3)], f"color ramp: {ramp}")
    [group] = doc["groups"]
    check(group["name"] == "Gain" and group["interface"][0]["in_out"] in ("INPUT", "OUTPUT"), "group interface")
    amount = next(i for i in group["interface"] if i["name"] == "Amount")
    check(amount["socket_type"] == "NodeSocketFloat" and amount["value"] == 0.5, f"interface default: {amount}")
    check(by_name["Group"]["properties"]["node_tree"] == "Gain", "group node names its tree")
    [image] = doc["images"]
    check(image["name"] == "wood_albedo" and image["source"] == "GENERATED" and image["filepath"] == ""
          and image["colorspace"] == "sRGB" and image["packed"] is False, f"image: {image}")
    text = graph.to_text(doc)
    check(text.endswith("\n") and graph.to_text(graph.dump([bpy.data.materials["Wood"]])) == text, "dump is deterministic")
    check(graph.from_text(text) == doc, "to_text and from_text invert")
    try:
        graph.from_text('{"format": "other", "version": 1}')
        check(False, "a foreign format must be refused")
    except ValueError:
        pass


def pytest_round(x):
    """Blender stores f32: compare through the same conversion the dumper does."""
    return float(__import__("array").array("f", [x])[0])


def test_round_trip(graph):
    original = graph.dump([bpy.data.materials["Wood"]])
    text = graph.to_text(original)
    # Load into fresh names: a loaded material is a new material unless `replace`.
    for material in list(bpy.data.materials):
        bpy.data.materials.remove(material)
    for group in list(bpy.data.node_groups):
        bpy.data.node_groups.remove(group)
    materials, warnings = graph.load(graph.from_text(text))
    check(warnings == [], f"load warnings: {warnings}")
    check([m.name for m in materials] == ["Wood"], f"loaded materials: {[m.name for m in materials]}")
    again = graph.dump(materials)
    if again != original:
        import difflib
        diff = difflib.unified_diff(graph.to_text(original).splitlines(), graph.to_text(again).splitlines(), lineterm="")
        check(False, "dump → load → dump is not the identity:\n" + "\n".join(list(diff)[:80]))
    # Replace rebuilds the existing material in place and keeps its users.
    cube = bpy.data.objects.new("Cube", bpy.data.meshes.new("Cube"))
    cube.data.materials.append(bpy.data.materials["Wood"])
    replaced, warnings = graph.load(graph.from_text(text), replace=True)
    check(replaced[0] is bpy.data.materials["Wood"] and cube.data.materials[0] is replaced[0], "replace keeps the material")
    check(len([m for m in bpy.data.materials if m.name.startswith("Wood")]) == 1, "replace makes no copy")
    check(graph.dump(replaced) == original, "a replaced material dumps the same")


def test_load_warnings(graph):
    doc = graph.from_text(graph.to_text(graph.dump([bpy.data.materials["Wood"]])))
    material = doc["materials"][0]
    material["name"] = "Broken"
    material["nodes"].append({"name": "Nope", "type": "ShaderNodeDoesNotExist", "label": "", "location": [0.0, 0.0],
                              "width": 140.0, "hide": False, "mute": False, "parent": None, "properties": {},
                              "inputs": [], "outputs": [], "internal_links": []})
    material["nodes"][0]["properties"]["no_such_property"] = 1
    material["nodes"][0]["inputs"].append({"identifier": "Nope", "name": "Nope", "type": "VALUE", "enabled": True, "value": 1.0})
    material["links"].append({"from": ["Nope", "Out"], "to": ["Principled BSDF", "Base Color"]})
    materials, warnings = graph.load(doc)
    check(len(materials) == 1 and materials[0].name == "Broken", "a material with problems still loads")
    check(any("unknown node type ShaderNodeDoesNotExist" in w for w in warnings), f"unknown node warning: {warnings}")
    check(any("no_such_property" in w for w in warnings), f"unknown property warning: {warnings}")
    check(any("Nope" in w and "input" in w for w in warnings), f"unknown input warning: {warnings}")
    check(any("link" in w for w in warnings), f"broken link warning: {warnings}")


def clear_data():
    for material in list(bpy.data.materials):
        bpy.data.materials.remove(material)
    for group in list(bpy.data.node_groups):
        bpy.data.node_groups.remove(group)


def round_trip(graph, materials, label):
    """Dump, empty the file, load: the loaded materials must dump the same, then again with replace."""
    original = graph.dump(materials)
    text = graph.to_text(original)
    clear_data()
    loaded, warnings = graph.load(graph.from_text(text))
    check(warnings == [], f"{label}: load warnings: {warnings}")
    check(graph.dump(loaded) == original, f"{label}: dump → load → dump is not the identity")
    replaced, warnings = graph.load(graph.from_text(text), replace=True)
    check(warnings == [], f"{label}: replace warnings: {warnings}")
    check(graph.dump(replaced) == original, f"{label}: replace → dump is not the identity")


def test_group_identifier_gap(graph):
    """Deleting an interface socket leaves a gap in the Socket_N numbers; a loaded group keeps them."""
    clear_data()
    group = bpy.data.node_groups.new("Gap", "ShaderNodeTree")
    items = [group.interface.new_socket(n, in_out="INPUT", socket_type="NodeSocketFloat") for n in ("A", "B", "C")]
    group.interface.remove(items[1])
    group.interface.new_socket("Out", in_out="OUTPUT", socket_type="NodeSocketFloat")
    gi = group.nodes.new("NodeGroupInput")
    go = group.nodes.new("NodeGroupOutput")
    group.links.new(gi.outputs["Socket_2"], go.inputs[0])
    check([i.identifier for i in group.interface.items_tree if i.in_out == "INPUT"] == ["Socket_0", "Socket_2"],
          "the test group has a gap")
    m = bpy.data.materials.new("Gappy")
    m.use_nodes = True
    t = m.node_tree
    principled = next(n for n in t.nodes if n.bl_idname == "ShaderNodeBsdfPrincipled")
    node = t.nodes.new("ShaderNodeGroup")
    node.node_tree = group
    for identifier in ("Socket_0", "Socket_2"):
        source = t.nodes.new("ShaderNodeValue")
        t.links.new(source.outputs[0], node.inputs[identifier])
    t.links.new(node.outputs[0], principled.inputs["Roughness"])
    round_trip(graph, [m], "gap")
    loaded = bpy.data.materials["Gappy"]
    group_links = [l for l in loaded.node_tree.links if "Group" in (l.from_node.name, l.to_node.name)]
    check(len(group_links) == 3, f"gap: links on the Group node after load: {len(group_links)}")


def test_stress_round_trip(graph):
    """Ramps, curve mappings, parents, a mix node, nested groups and two outputs survive dump → load → dump."""
    clear_data()
    m = bpy.data.materials.new("Stress")
    m.use_nodes = True
    t = m.node_tree
    ramp = t.nodes.new("ShaderNodeValToRGB").color_ramp
    ramp.color_mode, ramp.hue_interpolation, ramp.interpolation = "HSV", "FAR", "CONSTANT"
    ramp.elements[0].position, ramp.elements[1].position = 0.1, 0.9
    for position in (0.3, 0.5, 0.7):
        ramp.elements.new(position).color = (position, 1 - position, 0.5, 0.25)
    rgb = t.nodes.new("ShaderNodeRGBCurve").mapping
    rgb.use_clip, rgb.extend = False, "EXTRAPOLATED"
    rgb.curves[0].points.new(0.3, 0.6)
    rgb.curves[0].points.new(0.6, 0.2)
    rgb.curves[0].points[1].handle_type = "VECTOR"
    rgb.black_level, rgb.white_level = (0.1, 0.05, 0.0), (0.9, 0.95, 1.0)
    rgb.update()
    t.nodes.new("ShaderNodeFloatCurve").mapping.curves[0].points.new(0.5, 0.9)
    t.nodes.new("ShaderNodeVectorCurve")
    mix = t.nodes.new("ShaderNodeMix")
    mix.data_type, mix.blend_type, mix.clamp_factor = "RGBA", "SCREEN", False
    mix.inputs[0].default_value = 0.3
    frame = t.nodes.new("NodeFrame")
    frame.label, frame.location = "Hi", (100, 50)
    child = t.nodes.new("ShaderNodeMath")
    child.parent, child.location, child.hide, child.label = frame, (30, -20), True, "child"
    t.nodes.new("ShaderNodeTexNoise").noise_dimensions = "4D"
    tex = t.nodes.new("ShaderNodeTexImage")
    tex.image = bpy.data.images.new("stress_image", 4, 4)
    tex.image.colorspace_settings.name = "Non-Color"
    inner = bpy.data.node_groups.new("Inner", "ShaderNodeTree")
    for name in ("A", "B", "C"):
        inner.interface.new_socket(name, in_out="INPUT", socket_type="NodeSocketColor")
    inner.interface.new_socket("O", in_out="OUTPUT", socket_type="NodeSocketVector")
    inner.interface.move(inner.interface.items_tree[-1], 0)
    inner.interface.items_tree[3].default_value = (0.2, 0.3, 0.4, 1.0)
    inner.nodes.new("NodeGroupInput")
    inner.nodes.new("NodeGroupOutput")
    outer = bpy.data.node_groups.new("Outer", "ShaderNodeTree")
    outer.interface.new_socket("X", in_out="INPUT", socket_type="NodeSocketFloat")
    outer.nodes.new("ShaderNodeGroup").node_tree = inner
    t.nodes.new("ShaderNodeGroup").node_tree = outer
    t.nodes.new("ShaderNodeOutputMaterial").is_active_output = False
    colors = [s for s in mix.inputs if s.type == "RGBA"]
    t.links.new(t.nodes["Color Ramp"].outputs[0], colors[0])
    t.links.new(t.nodes["Color Ramp"].outputs[0], colors[1])
    round_trip(graph, [m], "stress")
    levels = bpy.data.materials["Stress"].node_tree.nodes["RGB Curves"].mapping
    check(close(levels.black_level, (0.1, 0.05, 0.0)) and close(levels.white_level, (0.9, 0.95, 1.0)),
          f"curve levels: {tuple(levels.black_level)} {tuple(levels.white_level)}")


def fixtures_module():
    spec = importlib.util.spec_from_file_location("make_fixtures", os.path.join(here, "make_fixtures.py"))
    make = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(make)
    return make


def test_replace_leaves_other_groups(graph):
    """Replace rebuilds a shader group of the document's name (keeping its fake user) but leaves another kind of node
    group of that name, and its users, alone."""
    clear_data()
    doc = graph.dump([fixtures_module().structure()])
    clear_data()
    geometry = bpy.data.node_groups.new("Gain", "GeometryNodeTree")
    holder = bpy.data.objects.new("Holder", bpy.data.meshes.new("Holder"))
    modifier = holder.modifiers.new("Nodes", "NODES")
    modifier.node_group = geometry
    materials, warnings = graph.load(doc, replace=True)
    kept = bpy.data.node_groups.get("Gain")
    check(kept is not None and kept.bl_idname == "GeometryNodeTree", "replace removed the Geometry Nodes group")
    used = modifier.node_group
    check(used is not None and used.name == "Gain" and used.bl_idname == "GeometryNodeTree",
          f"the modifier lost its group: {used and (used.name, used.bl_idname)}")
    check(warnings == ["group Gain: an existing non-shader node group of that name was left alone"],
          f"replace over a non-shader group: {warnings}")
    tree = materials[0].node_tree.nodes["Gain"].node_tree
    check(tree is not None and tree.bl_idname == "ShaderNodeTree" and tree.name != "Gain",
          f"the loaded group node's tree: {tree and (tree.name, tree.bl_idname)}")
    clear_data()
    bpy.data.node_groups.new("Gain", "ShaderNodeTree").use_fake_user = True
    graph.load(doc, replace=True)
    replaced = bpy.data.node_groups["Gain"]
    check(replaced.use_fake_user and len(replaced.nodes) == 3, "replace keeps the old group's fake user")


def test_muted_link(graph):
    """A muted link is dumped muted and loads muted; every link says whether Blender finds it valid."""
    clear_data()
    m = bpy.data.materials.new("MutedLink")
    m.use_nodes = True
    t = m.node_tree
    principled = next(n for n in t.nodes if n.bl_idname == "ShaderNodeBsdfPrincipled")
    value = t.nodes.new("ShaderNodeValue")
    t.links.new(value.outputs[0], principled.inputs["Roughness"]).is_muted = True
    links = graph.dump([m])["materials"][0]["links"]
    check([link.get("muted") for link in links].count(True) == 1 and all(link.get("valid") is True for link in links),
          f"links carry muted and valid: {links}")
    round_trip(graph, [m], "muted link")
    loaded = bpy.data.materials["MutedLink"].node_tree.links
    check([link.is_muted for link in loaded if link.from_node.bl_idname == "ShaderNodeValue"] == [True],
          "the link loads muted")


def test_legacy_identifiers(graph):
    """A group saved before Blender 4.0 names its interface sockets Input_N and Output_N: the loaded group's sockets get
    the identifiers Blender gives now, and the group nodes, Group Input/Output nodes and links naming the old ones
    find them."""
    clear_data()
    material = fixtures_module().structure()
    material.node_tree.nodes["Gain"].inputs[0].default_value = 0.75
    original = graph.dump([material])
    text = graph.to_text(original).replace('"Socket_0"', '"Input_0"').replace('"Socket_1"', '"Output_1"')
    check('"Input_0"' in text and '"Output_1"' in text, "the legacy document names Input_0 and Output_1")
    clear_data()
    materials, warnings = graph.load(graph.from_text(text))
    check(warnings == [], f"legacy identifiers: load warnings: {warnings}")
    tree = materials[0].node_tree
    links = original["materials"][0]["links"]
    check(len(tree.links) == len(links), f"legacy identifiers: {len(tree.links)} links, not {len(links)}")
    gain = tree.nodes["Gain"]
    check(gain.node_tree is not None and len(gain.node_tree.links) == len(original["groups"][0]["links"]),
          "legacy identifiers: the group's inner links")
    check(abs(gain.inputs["Amount"].default_value - 0.75) < 1e-6,
          f"legacy identifiers: Amount is {gain.inputs['Amount'].default_value}")
    check(any(link.from_node == gain for link in tree.links), "legacy identifiers: the group node's output link")


def test_engine_output(graph):
    """A material whose only output targets one engine names that output."""
    clear_data()
    m = bpy.data.materials.new("EeveeOnly")
    m.use_nodes = True
    output = next(n for n in m.node_tree.nodes if n.bl_idname == "ShaderNodeOutputMaterial")
    output.target = "EEVEE"
    check(graph.dump([m])["materials"][0]["output"] == output.name, "an EEVEE-only output is the output")


def test_document_errors(graph):
    """A NaN cannot be written; a malformed document is an import error, not an exception."""
    clear_data()
    m = bpy.data.materials.new("Nan")
    m.use_nodes = True
    m.node_tree.nodes.new("ShaderNodeMath").inputs[1].default_value = float("nan")
    try:
        graph.to_text(graph.dump([m]))
        check(False, "a NaN value must be refused")
    except ValueError as error:
        check("NaN" in str(error) and "Nan" in str(error), f"the NaN error names the material: {error}")
    m.node_tree.nodes.remove(m.node_tree.nodes["Math"])
    doc = graph.dump([m])
    del doc["materials"][0]["nodes"][0]["width"]
    with tempfile.TemporaryDirectory() as folder:
        path = os.path.join(folder, "broken.json")
        with open(path, "w", encoding="utf-8") as f:
            f.write(graph.to_text(doc))
        # An operator's error report reaches a Python caller as a RuntimeError; an uncaught exception would too, as
        # a traceback.
        try:
            result = str(bpy.ops.shadewire.import_graph(filepath=path))
        except RuntimeError as error:
            result = str(error)
        check("malformed document" in result and "Traceback" not in result, f"a malformed document: {result}")


PIXELS = [0.5, 0.25, 0.75, 1.0, 0.1, 0.2, 0.3, 1.0, 0.9, 0.8, 0.7, 1.0, 0.0, 1.0, 0.5, 0.5]


def write_png(path, pixels):
    """A 2x2 PNG file with these pixels."""
    image = bpy.data.images.new("written", 2, 2, alpha=True)
    image.pixels = pixels
    image.filepath_raw = path
    image.file_format = "PNG"
    image.save()
    bpy.data.images.remove(image)


def close(a, b):
    """Equal element by element within one 8-bit step."""
    return len(a) == len(b) and all(abs(x - y) <= 1 / 255 for x, y in zip(a, b))


def test_operators():
    with tempfile.TemporaryDirectory() as folder:
        path = os.path.join(folder, "wood.json")
        bpy.ops.wm.read_homefile(use_empty=True)
        m = textured_material()
        albedo = bpy.data.images["wood_albedo"]
        # Opaque: an image made without alpha is saved without it (Blender then multiplies the color by the alpha).
        albedo.pixels = [1.0 if i % 4 == 3 else x for i, x in enumerate(PIXELS)] * 4
        painted = bpy.data.images.new("painted", 2, 2, alpha=True)
        painted.pixels = PIXELS
        painted.pack()
        m.node_tree.nodes.new("ShaderNodeTexImage").image = painted
        # A float image with linear pixels: an 8-bit PNG would hold them sRGB encoded, read back as linear.
        linear = bpy.data.images.new("linear", 2, 2, alpha=True, float_buffer=True)
        linear.colorspace_settings.name = "Linear Rec.709"
        linear.pixels = [1.0 if i % 4 == 3 else x for i, x in enumerate(PIXELS)]
        m.node_tree.nodes.new("ShaderNodeTexImage").image = linear
        # Two files of one basename in two folders, and a second image of the first file.
        sources = []
        for sub, shift in (("a", 0.0), ("b", 0.5)):
            os.makedirs(os.path.join(folder, sub))
            source = os.path.join(folder, sub, "twin.png")
            write_png(source, [min(1.0, x + shift) for x in PIXELS])
            sources.append(source)
        for source in (sources[0], sources[1], sources[0]):
            m.node_tree.nodes.new("ShaderNodeTexImage").image = bpy.data.images.load(source, check_existing=False)
        expected = {"wood_albedo": albedo.pixels[:], "painted": painted.pixels[:], "linear": linear.pixels[:]}
        cube = bpy.data.objects.new("Cube", bpy.data.meshes.new("Cube"))
        cube.data.materials.append(m)
        bpy.context.scene.collection.objects.link(cube)
        cube.select_set(True)
        bpy.context.view_layer.objects.active = cube
        result = bpy.ops.shadewire.export_graph(filepath=path, scope="SELECTED", copy_textures=True)
        check(result == {"FINISHED"}, f"export: {result}")
        doc = json.load(open(path, encoding="utf-8"))
        check([x["name"] for x in doc["materials"]] == ["Wood"], "exported the selected object's material")
        texture = os.path.join(folder, "textures", "wood_albedo.png")
        check(os.path.isfile(texture), "copy_textures wrote the generated image")
        check(doc["images"][0]["filepath"].replace("\\", "/") == texture.replace("\\", "/"), f"filepath rewritten: {doc['images'][0]}")
        paths = {entry["name"]: entry["filepath"] for entry in doc["images"]}
        check(paths["painted"].endswith("textures/painted.png") and os.path.isfile(paths["painted"]),
              f"the packed image is written from its bytes: {paths['painted']}")
        twins = [paths["twin.png"], paths["twin.png.001"], paths["twin.png.002"]]
        check(twins[0].endswith("textures/twin.png") and twins[1].endswith("textures/twin.1.png")
              and twins[2] == twins[0], f"one basename from two folders: {twins}")
        for source, written in zip(sources, twins):
            check(os.path.isfile(written) and open(source, "rb").read() == open(written, "rb").read(),
                  f"{written} is a copy of {source}")
        written = sorted(os.listdir(os.path.join(folder, "textures")))
        check(written == ["linear.exr", "painted.png", "twin.1.png", "twin.png", "wood_albedo.png"],
              f"textures written: {written}")
        bpy.ops.wm.read_homefile(use_empty=True)
        result = bpy.ops.shadewire.import_graph(filepath=path)
        check(result == {"FINISHED"} and "Wood" in bpy.data.materials, f"import: {result}")
        check(bpy.data.images["wood_albedo"].filepath.replace("\\", "/").endswith("textures/wood_albedo.png"), "image loaded from the copied file")
        for name, pixels in expected.items():
            back = bpy.data.images[name].pixels[:]
            wrong = [(i, round(x, 3), round(y, 3)) for i, (x, y) in enumerate(zip(back, pixels)) if abs(x - y) > 1 / 255]
            check(close(back, pixels), f"{name} pixels after copy and load (index, read, expected): {wrong[:8]}")


def test_fixtures_are_current(graph):
    """Regenerating the fixtures in this Blender changes nothing but the producer line."""
    make = fixtures_module()
    folder = os.path.join(here, "fixtures", "graphs")
    for name, build in make.FIXTURES.items():
        bpy.ops.wm.read_homefile(use_empty=True)
        doc = graph.dump([build()])
        path = os.path.join(folder, f"{name}.json")
        check(os.path.isfile(path), f"fixture {name}.json missing: run make_fixtures.py")
        if os.path.isfile(path):
            on_disk = json.load(open(path, encoding="utf-8"))
            on_disk["producer"] = doc["producer"]
            check(on_disk == doc, f"fixture {name}.json is stale: run make_fixtures.py")
        clear_data()  # a loaded material keeps its name only when the built one is gone
        materials, warnings = graph.load(doc)
        check(warnings == [], f"{name}: load warnings {warnings}")
        check(graph.dump(materials) == doc, f"{name}: dump → load → dump differs")


bpy.ops.wm.read_homefile(use_empty=True)
addon = load_addon()
test_dump_shape(addon.graph)
test_round_trip(addon.graph)
test_load_warnings(addon.graph)
test_group_identifier_gap(addon.graph)
test_stress_round_trip(addon.graph)
test_replace_leaves_other_groups(addon.graph)
test_muted_link(addon.graph)
test_legacy_identifiers(addon.graph)
test_engine_output(addon.graph)
test_document_errors(addon.graph)
test_operators()
test_fixtures_are_current(addon.graph)
print("FAILURES", len(failures))
sys.exit(1 if failures else 0)
