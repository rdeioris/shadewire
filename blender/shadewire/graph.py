# SPDX-FileCopyrightText: 2026 Roberto De Ioris
# SPDX-License-Identifier: GPL-3.0-or-later

"""Materials' node trees as a JSON document, exactly as Blender holds them.

The document speaks Blender's vocabulary: a node is its `bl_idname`, a socket its identifier, a property its RNA
identifier, an enum its identifier string. Nothing here interprets a node: no meaning is given to a Mix node's data
type or a Math node's operation, every property and socket value is written as it is, so a reader (the Shadewire
compiler front end) decides what a node means and the document stays the same whatever that reading becomes.
"""

import json

import bpy

FORMAT = "shadewire-blender-graph"
VERSION = 1

# Properties every shader node has; a node's own properties are the ones past these.
BASE_PROPERTIES = {p.identifier for p in bpy.types.ShaderNode.bl_rna.properties}

SETTINGS = ("surface_render_method", "blend_method", "use_backface_culling", "displacement_method")

# Read-only pointers whose content is still the node's own state (the pointer is fixed, the struct is edited).
STRUCTS = ("color_ramp", "mapping")

# Socket types that carry a `default_value` in the document.
VALUE_SOCKETS = {"VALUE", "INT", "BOOLEAN", "VECTOR", "ROTATION", "RGBA", "STRING"}


class Collector:
    """The groups and images met while walking, in first-use order."""

    def __init__(self):
        self.groups = []
        self.images = []
        self.group_names = set()
        self.image_names = set()

    def image(self, image):
        """Record an image and return its name."""
        if image.name not in self.image_names:
            self.image_names.add(image.name)
            self.images.append(dump_image(image))
        return image.name

    def group(self, tree):
        """Record a node group (and the groups inside it) and return its name."""
        if tree.name not in self.group_names:
            self.group_names.add(tree.name)
            entry = {"name": tree.name}
            self.groups.append(entry)
            entry.update(dump_group(tree, self))
        return tree.name


def dump(materials):
    """The document of the given materials, with the node groups and images they use."""
    collector = Collector()
    entries = [dump_material(material, collector) for material in materials]
    return {
        "format": FORMAT,
        "version": VERSION,
        "producer": f"Blender {bpy.app.version_string}",
        "materials": entries,
        "groups": collector.groups,
        "images": collector.images,
    }


def dump_material(material, collector):
    """One material: its settings, output node, nodes and links."""
    tree = material.node_tree
    output = tree.get_output_node("ALL") if tree else None
    return {
        "name": material.name,
        "settings": {key: getattr(material, key) for key in SETTINGS},
        "output": output.name if output else None,
        "nodes": [dump_node(node, collector) for node in tree.nodes] if tree else [],
        "links": dump_links(tree) if tree else [],
    }


def dump_node(node, collector):
    """One node: placement, own properties, sockets and internal links."""
    return {
        "name": node.name,
        "type": node.bl_idname,
        "label": node.label,
        "location": [float(node.location.x), float(node.location.y)],
        "width": float(node.width),
        "hide": node.hide,
        "mute": node.mute,
        "parent": node.parent.name if node.parent else None,
        "properties": dump_properties(node, collector),
        "inputs": [dump_socket(socket) for socket in node.inputs],
        "outputs": [dump_socket(socket) for socket in node.outputs],
        "internal_links": [[link.from_socket.identifier, link.to_socket.identifier] for link in node.internal_links],
    }


def dump_properties(node, collector):
    """The node's own writable properties, by RNA identifier."""
    result = {}
    for prop in node.bl_rna.properties:
        if prop.identifier in BASE_PROPERTIES or (prop.is_readonly and prop.identifier not in STRUCTS):
            continue
        value = getattr(node, prop.identifier)
        if prop.type == "POINTER":
            result[prop.identifier] = dump_pointer(prop.identifier, value, collector)
        elif prop.type == "ENUM":
            result[prop.identifier] = sorted(value) if prop.is_enum_flag else value
        elif prop.type in ("BOOLEAN", "INT", "FLOAT", "STRING"):
            result[prop.identifier] = plain(value) if getattr(prop, "is_array", False) else value
    return result


def plain(value):
    """A vector, color or matrix as nested lists of floats (integers and booleans stay as they are)."""
    if isinstance(value, (bool, int, float, str)):
        return float(value) if isinstance(value, float) else value
    return [plain(item) for item in value]


def dump_pointer(identifier, value, collector):
    """A pointer property: images and groups by name (recorded), ramps and curves in full, others by name."""
    if value is None:
        return None
    if identifier == "image":
        return collector.image(value)
    if identifier == "node_tree":
        return collector.group(value)
    if identifier == "color_ramp":
        return dump_ramp(value)
    if identifier == "mapping":
        return dump_mapping(value)
    return getattr(value, "name", None)


def dump_ramp(ramp):
    """A color ramp: its modes and elements."""
    return {
        "interpolation": ramp.interpolation,
        "color_mode": ramp.color_mode,
        "hue_interpolation": ramp.hue_interpolation,
        "elements": [{"position": float(e.position), "color": plain(e.color)} for e in ramp.elements],
    }


def dump_mapping(mapping):
    """A curve mapping: its clipping and the points of each curve."""
    return {
        "extend": mapping.extend,
        "tone": mapping.tone,
        "clip_min_x": float(mapping.clip_min_x),
        "clip_min_y": float(mapping.clip_min_y),
        "clip_max_x": float(mapping.clip_max_x),
        "clip_max_y": float(mapping.clip_max_y),
        "use_clip": mapping.use_clip,
        "curves": [
            [{"location": plain(point.location), "handle_type": point.handle_type} for point in curve.points]
            for curve in mapping.curves
        ],
    }


def dump_socket(socket):
    """One socket: identity, enabled state and, when it has one, its default value."""
    result = {"identifier": socket.identifier, "name": socket.name, "type": socket.type, "enabled": socket.enabled}
    value = socket_value(socket)
    if value is not None:
        result["value"] = value
    return result


def socket_value(socket):
    """The socket's `default_value` as JSON, or None when it has none (shader and virtual sockets)."""
    if socket.type not in VALUE_SOCKETS or not hasattr(socket, "default_value"):
        return None
    return plain(socket.default_value)


def dump_links(tree):
    """The tree's links by node name and socket identifier, the invalid ones too."""
    return [
        {"from": [link.from_node.name, link.from_socket.identifier],
         "to": [link.to_node.name, link.to_socket.identifier]}
        for link in tree.links
    ]


def dump_group(tree, collector):
    """A node group: its interface sockets, nodes and links."""
    interface = []
    for item in tree.interface.items_tree:
        if item.item_type != "SOCKET":
            continue
        entry = {"identifier": item.identifier, "name": item.name, "in_out": item.in_out,
                 "socket_type": item.socket_type}
        if item.in_out == "INPUT" and hasattr(item, "default_value"):
            entry["value"] = plain(item.default_value)
        interface.append(entry)
    return {
        "interface": interface,
        "nodes": [dump_node(node, collector) for node in tree.nodes],
        "links": dump_links(tree),
    }


def dump_image(image):
    """An image: where it comes from and how it is read."""
    path = bpy.path.abspath(image.filepath) if image.filepath else ""
    return {
        "name": image.name,
        "filepath": path.replace("\\", "/"),
        "colorspace": image.colorspace_settings.name,
        "alpha_mode": image.alpha_mode,
        "source": image.source,
        "packed": image.packed_file is not None,
    }


def to_text(doc):
    """The document as text: two-space indent, insertion order, trailing newline."""
    return json.dumps(doc, indent=2) + "\n"


def from_text(text):
    """Parse a document, refusing one that is not this format and version."""
    doc = json.loads(text)
    if not isinstance(doc, dict) or doc.get("format") != FORMAT:
        raise ValueError(f"not a {FORMAT} document")
    if doc.get("version") != VERSION:
        raise ValueError(f"unsupported {FORMAT} version {doc.get('version')!r} (this add-on reads version {VERSION})")
    return doc


class Loader:
    """What loading one document needs: the warnings so far, the document's images, the groups and images made."""

    def __init__(self, doc, replace):
        self.replace = replace
        self.warnings = []
        self.image_docs = {entry["name"]: entry for entry in doc.get("images", [])}
        self.images = {}
        self.groups = {}

    def warn(self, where, message):
        self.warnings.append(f"{where}: {message}")


def load(doc, replace=False):
    """Instantiate a document: its node groups, then its materials. Returns (materials, warnings).

    A material or group that already has the name is a new one (Blender numbers it) unless `replace`, which rebuilds
    the existing one in place. Whatever cannot be set, an unknown node type, property, socket or link, is skipped and
    named in the warnings, so a document from another Blender version still loads as far as it can.
    """
    loader = Loader(doc, replace)
    entries = doc.get("groups", [])
    # Every group exists before any is filled: a group may hold another that comes later in the document.
    for entry in entries:
        loader.groups[entry["name"]] = make_group(entry, loader)
    for entry in entries:
        load_tree(loader.groups[entry["name"]], entry, f"group {entry['name']}", loader)
    materials = [load_material(entry, loader) for entry in doc.get("materials", [])]
    return materials, loader.warnings


def make_group(entry, loader):
    """A node group with the entry's name and interface (empty of nodes).

    Replacing builds a new group and hands the old one's users to it: an emptied interface would keep counting its
    socket identifiers from where it stopped, and the document's identifiers would no longer be the ones Blender gives.
    """
    old = bpy.data.node_groups.get(entry["name"]) if loader.replace else None
    tree = bpy.data.node_groups.new(entry["name"], "ShaderNodeTree")
    if old is not None:
        old.user_remap(tree)
        bpy.data.node_groups.remove(old)
        tree.name = entry["name"]
    load_interface(tree, entry.get("interface", []), f"group {entry['name']}", loader)
    return tree


def load_interface(tree, items, where, loader):
    """The group's interface sockets, created in the order Blender numbered them so the identifiers match."""
    order = sorted(range(len(items)), key=lambda i: (identifier_number(items[i]["identifier"]), i))
    created = {}
    for index in order:
        item = items[index]
        try:
            socket = tree.interface.new_socket(item["name"], in_out=item["in_out"], socket_type=item["socket_type"])
            if item["in_out"] == "INPUT" and "value" in item:
                socket.default_value = item["value"]
        except Exception as error:
            loader.warn(where, f"interface {item['name']}: {error}")
            continue
        if socket.identifier != item["identifier"]:
            loader.warn(where, f"interface {item['name']} is {socket.identifier}, not {item['identifier']}")
        created[index] = socket
    # Creation order is not the document's order when the identifiers were not numbered in it.
    for position, socket in enumerate(created[index] for index in sorted(created)):
        if tree.interface.items_tree[position] != socket:
            tree.interface.move(socket, position)


def identifier_number(identifier):
    """The number at the end of an interface identifier (`Socket_3` is 3); identifiers without one go last."""
    tail = identifier.rpartition("_")[2]
    return int(tail) if tail.isdigit() else float("inf")


def load_material(entry, loader):
    """One material: its settings, then its node tree."""
    material = bpy.data.materials.get(entry["name"]) if loader.replace else None
    if material is None:
        material = bpy.data.materials.new(entry["name"])
    material.use_nodes = True
    for key, value in entry.get("settings", {}).items():
        try:
            setattr(material, key, value)
        except Exception as error:
            loader.warn(material.name, f"setting {key}: {error}")
    load_tree(material.node_tree, entry, material.name, loader)
    return material


def load_tree(tree, entry, where, loader):
    """Rebuild a node tree from the entry: nodes, then parents and places, then links, then the active output."""
    tree.nodes.clear()
    nodes = {}
    for node_entry in entry.get("nodes", []):
        node = load_node(tree, node_entry, where, loader)
        if node is not None:
            nodes[node_entry["name"]] = node
    # Parents once every node exists; a node's location is relative to its frame, so it is set after the parent.
    for node_entry in entry.get("nodes", []):
        node = nodes.get(node_entry["name"])
        if node is None:
            continue
        parent = node_entry.get("parent")
        if parent is not None:
            if parent in nodes:
                node.parent = nodes[parent]
            else:
                loader.warn(where, f"{node.name}: no parent {parent}")
        node.location = node_entry["location"]
    load_links(tree, entry.get("links", []), nodes, where, loader)
    output = nodes.get(entry.get("output"))
    if output is not None and hasattr(output, "is_active_output"):
        output.is_active_output = True


def load_node(tree, entry, where, loader):
    """Create one node and set what the entry says; None when its type is unknown."""
    try:
        node = tree.nodes.new(entry["type"])
    except Exception:
        loader.warn(where, f"unknown node type {entry['type']}, skipped")
        return None
    node.name = entry["name"]
    # Properties first: they decide which sockets are there and enabled.
    set_properties(node, entry.get("properties", {}), where, loader)
    node.label = entry.get("label", "")
    node.width = entry["width"]
    node.hide = entry["hide"]
    node.mute = entry["mute"]
    set_inputs(node, entry.get("inputs", []), where, loader)
    return node


def set_properties(node, properties, where, loader):
    """Each property by RNA identifier; unknown, read-only and rejected ones become warnings."""
    for key, value in properties.items():
        prop = node.bl_rna.properties.get(key)
        if prop is None or key in BASE_PROPERTIES or (prop.is_readonly and key not in STRUCTS):
            loader.warn(where, f"{node.name}: no property {key}")
            continue
        try:
            if prop.type == "POINTER":
                set_pointer(node, prop, value, where, loader)
            elif prop.type == "ENUM" and prop.is_enum_flag:
                setattr(node, key, set(value))
            else:
                setattr(node, key, value)
        except Exception as error:
            loader.warn(where, f"{node.name}: property {key}: {error}")


def set_pointer(node, prop, value, where, loader):
    """A pointer property from its document form: ramps and curves in place, the rest by the pointee's name."""
    key = prop.identifier
    if key == "color_ramp":
        if value is not None:
            set_ramp(node.color_ramp, value)
    elif key == "mapping":
        if value is not None:
            set_mapping(node.mapping, value)
    elif value is None:
        setattr(node, key, None)
    elif key == "image":
        image = resolve_image(value, where, loader)
        if image is not None:
            node.image = image
    elif key == "node_tree":
        tree = loader.groups.get(value) or bpy.data.node_groups.get(value)
        if tree is None:
            loader.warn(where, f"{node.name}: no node group {value}")
        else:
            node.node_tree = tree
    else:
        # Any other pointer names an ID of the type the property points to.
        for collection in bpy.data.bl_rna.properties:
            if collection.type == "COLLECTION" and collection.fixed_type == prop.fixed_type:
                pointee = getattr(bpy.data, collection.identifier).get(value)
                if pointee is not None:
                    setattr(node, key, pointee)
                    return
        loader.warn(where, f"{node.name}: property {key}: no {prop.fixed_type.identifier} named {value}")


def resolve_image(name, where, loader):
    """The image of that name: the one in the file, else loaded from the document's filepath; None with no file."""
    if name in loader.images:
        return loader.images[name]
    entry = loader.image_docs.get(name, {})
    image = bpy.data.images.get(name)
    if image is None and entry.get("filepath"):
        try:
            image = bpy.data.images.load(entry["filepath"], check_existing=True)
            if image.name != name:
                image.name = name
        except Exception as error:
            loader.warn(where, f"image {name}: {error}")
    elif image is None:
        loader.warn(where, f"image {name} has no file")
    if image is not None:
        try:
            image.colorspace_settings.name = entry.get("colorspace", image.colorspace_settings.name)
            image.alpha_mode = entry.get("alpha_mode", image.alpha_mode)
        except Exception as error:
            loader.warn(where, f"image {name}: {error}")
    loader.images[name] = image
    return image


def set_ramp(ramp, spec):
    """Write a color ramp back: its modes, then its elements one by one (Blender keeps at least one)."""
    ramp.interpolation = spec["interpolation"]
    ramp.color_mode = spec["color_mode"]
    ramp.hue_interpolation = spec["hue_interpolation"]
    elements = spec["elements"]
    while len(ramp.elements) > max(len(elements), 1):
        ramp.elements.remove(ramp.elements[-1])
    for index, entry in enumerate(elements):
        if index < len(ramp.elements):
            element = ramp.elements[index]
            element.position = entry["position"]
        else:
            element = ramp.elements.new(entry["position"])
        element.color = entry["color"]


def set_mapping(mapping, spec):
    """Write a curve mapping back: its clipping, then the points of each curve (Blender keeps at least two)."""
    for key in ("extend", "tone", "clip_min_x", "clip_min_y", "clip_max_x", "clip_max_y", "use_clip"):
        setattr(mapping, key, spec[key])
    for curve, points in zip(mapping.curves, spec["curves"]):
        while len(curve.points) > max(len(points), 2):
            curve.points.remove(curve.points[-1])
        for index, entry in enumerate(points):
            if index < len(curve.points):
                curve.points[index].location = entry["location"]
            else:
                curve.points.new(*entry["location"])
        for point, entry in zip(curve.points, points):
            point.handle_type = entry["handle_type"]
    mapping.update()


def set_inputs(node, inputs, where, loader):
    """Input values by socket identifier; an identifier the node does not have is a warning."""
    sockets = {socket.identifier: socket for socket in reversed(node.inputs)}
    for entry in inputs:
        socket = sockets.get(entry["identifier"])
        if socket is None:
            loader.warn(where, f"{node.name}: no input {entry['identifier']}")
        elif "value" in entry:
            try:
                socket.default_value = entry["value"]
            except Exception as error:
                loader.warn(where, f"{node.name}: input {entry['identifier']}: {error}")


def load_links(tree, links, nodes, where, loader):
    """Links by node name and socket identifier; one whose node or socket is missing is skipped with a warning."""
    for link in links:
        (from_name, from_id), (to_name, to_id) = link["from"], link["to"]
        label = f"link {from_name}.{from_id} -> {to_name}.{to_id} skipped"
        if from_name not in nodes or to_name not in nodes:
            loader.warn(where, f"{label}: unknown node")
            continue
        source = next((s for s in nodes[from_name].outputs if s.identifier == from_id), None)
        target = next((s for s in nodes[to_name].inputs if s.identifier == to_id), None)
        if source is None or target is None:
            loader.warn(where, f"{label}: no socket {from_id if source is None else to_id}")
            continue
        try:
            tree.links.new(source, target, verify_limits=False)
        except Exception as error:
            loader.warn(where, f"{label}: {error}")
