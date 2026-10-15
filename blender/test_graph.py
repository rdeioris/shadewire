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
    check({"from": ["Albedo", "Color"], "to": ["Principled BSDF", "Base Color"]} in material["links"], "link by identifiers")
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


def test_operators():
    with tempfile.TemporaryDirectory() as folder:
        path = os.path.join(folder, "wood.json")
        bpy.ops.wm.read_homefile(use_empty=True)
        m = textured_material()
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
        bpy.ops.wm.read_homefile(use_empty=True)
        result = bpy.ops.shadewire.import_graph(filepath=path)
        check(result == {"FINISHED"} and "Wood" in bpy.data.materials, f"import: {result}")
        check(bpy.data.images["wood_albedo"].filepath.replace("\\", "/").endswith("textures/wood_albedo.png"), "image loaded from the copied file")


bpy.ops.wm.read_homefile(use_empty=True)
addon = load_addon()
test_dump_shape(addon.graph)
test_round_trip(addon.graph)
test_load_warnings(addon.graph)
test_group_identifier_gap(addon.graph)
test_stress_round_trip(addon.graph)
test_operators()
print("FAILURES", len(failures))
sys.exit(1 if failures else 0)
