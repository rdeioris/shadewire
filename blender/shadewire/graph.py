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
