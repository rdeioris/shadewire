"""Builds a Cycles node tree from a Shadewire IR document (`shadewire ir --json`).

Values are lists of components; a component is a Python float (a constant, folded on the spot) or a node
output socket. Ops work component by component with Math nodes (vector ops use Vector Math), so every
Shadewire op has an exact node form except the noises, which use Blender's own noise textures.

The closure tree becomes Cycles shaders: each lobe's weight (the product of the mixes, multiplies and layers
above it) goes into the lobe's color when the lobe scales by one, else through a Mix Shader with black. A
`layer` passes its base what the top does not reflect (one minus the top's albedo), as MaterialX's own
GLSL does; the lobes are summed with Add Shader.
"""

import math
import os

import bpy

WIDTH = {"float": 1, "integer": 1, "boolean": 1, "vector2": 2, "vector3": 3, "color3": 3, "vector4": 4, "color4": 4}
COLUMN = 220
ROW = 160


class Builder:
    def __init__(self, material, doc, base_dir):
        self.material = material
        self.doc = doc
        self.base_dir = base_dir
        material.use_nodes = True
        self.tree = material.node_tree
        self.tree.nodes.clear()
        self.warnings = []
        self.values = []
        self.split_cache = {}
        self.single = {}
        self.depth = {}
        self.column_rows = {}
        self.black = None

    # ------------------------------------------------------------------ nodes and layout

    def warn(self, message):
        if message not in self.warnings:
            self.warnings.append(message)

    def node(self, kind, inputs=(), **props):
        """A new node; `inputs` are (socket key, component) pairs; its column follows its deepest input."""
        n = self.tree.nodes.new(kind)
        for key, value in props.items():
            setattr(n, key, value)
        depth = 0
        for key, value in inputs:
            depth = max(depth, self.set_input(n.inputs[key], value))
        self.place(n, depth + 1)
        return n

    def place(self, n, depth):
        self.depth[n.name] = depth
        row = self.column_rows.get(depth, 0)
        self.column_rows[depth] = row + 1
        n.location = (depth * COLUMN, -row * ROW)
        n.hide = n.bl_idname.startswith("ShaderNodeMath") or n.bl_idname in ("ShaderNodeSeparateXYZ", "ShaderNodeCombineXYZ")

    def set_input(self, socket, value):
        """Links a socket or sets a constant (float, or a 3/4 list for vector and color sockets); its depth."""
        if isinstance(value, (int, float)):
            if socket.type in ("VECTOR", "RGBA"):
                n = len(socket.default_value)
                socket.default_value = [float(value)] * n if n != 4 else [float(value)] * 3 + [1.0]
            else:
                socket.default_value = float(value)
            return 0
        if isinstance(value, (list, tuple)):
            n = len(socket.default_value)
            vals = list(value)[:n] + [1.0] * max(0, n - len(value))
            socket.default_value = vals
            return 0
        self.tree.links.new(value, socket)
        return self.depth.get(value.node.name, 0)

    # ------------------------------------------------------------------ component math

    def math(self, op, *args, **props):
        if all(isinstance(a, (int, float)) for a in args):
            folded = fold(op, [float(a) for a in args])
            if folded is not None:
                return folded
        n = self.node("ShaderNodeMath", [(k, a) for k, a in enumerate(args)], operation=op, **props)
        return n.outputs[0]

    def add(self, a, b):
        if b == 0.0:
            return a
        if a == 0.0:
            return b
        return self.math("ADD", a, b)

    def sub(self, a, b):
        return a if b == 0.0 else self.math("SUBTRACT", a, b)

    def mul(self, a, b):
        if a == 1.0:
            return b
        if b == 1.0:
            return a
        if a == 0.0 or b == 0.0:
            return 0.0
        return self.math("MULTIPLY", a, b)

    def lerp(self, bg, fg, t):
        """bg + (fg - bg) * t."""
        if t == 0.0:
            return bg
        if t == 1.0:
            return fg
        if bg == 0.0:
            return self.mul(fg, t)
        return self.math("MULTIPLY_ADD", self.sub(fg, bg), t, bg)

    def split(self, socket, width=3):
        """The components of a vector or color socket."""
        key = (socket.node.name, socket.identifier)
        if key not in self.split_cache:
            n = self.node("ShaderNodeSeparateXYZ", [("Vector", socket)])
            self.split_cache[key] = [n.outputs[0], n.outputs[1], n.outputs[2]]
        return self.split_cache[key][:width]

    def vector(self, comps):
        """A vector socket (or a constant list) holding up to three components."""
        comps = list(comps) + [0.0] * (3 - len(comps))
        if all(isinstance(c, (int, float)) for c in comps[:3]):
            return [float(c) for c in comps[:3]]
        n = self.node("ShaderNodeCombineXYZ", [(k, c) for k, c in enumerate(comps[:3])])
        return n.outputs[0]

    def vmath(self, op, *vectors):
        """Vector Math over three-component values; returns the node."""
        return self.node("ShaderNodeVectorMath", [(k, self.vector(v)) for k, v in enumerate(vectors)], operation=op)

    # ------------------------------------------------------------------ values

    def value(self, index):
        return self.values[index]

    def widen(self, comps, width):
        """A scalar broadcast to `width` components."""
        if len(comps) == 1 and width > 1:
            return comps * width
        return comps[:width] if len(comps) >= width else comps + [1.0] * (width - len(comps))

    def constant(self, node):
        v = node.get("value")
        if isinstance(v, bool):
            return [1.0 if v else 0.0]
        if isinstance(v, (int, float)):
            return [float(v)]
        return [float(x) for x in v]

    def uniform(self, node):
        """An editable value: a Value node per scalar, an RGB node for colors."""
        vals = self.constant(node)
        label = node.get("name", "")
        if node["type"] in ("color3", "color4"):
            n = self.node("ShaderNodeRGB")
            n.outputs[0].default_value = vals[:3] + [1.0]
            n.label = label
            n.hide = False
            comps = self.split(n.outputs[0])
            return comps + vals[3:4]
        comps = []
        for k, x in enumerate(vals):
            n = self.node("ShaderNodeValue")
            n.outputs[0].default_value = x
            n.label = label if len(vals) == 1 else f"{label}[{k}]"
            n.hide = False
            comps.append(n.outputs[0])
        return comps

    def context(self, node):
        read = node["read"]
        space = node.get("space", "world")
        width = WIDTH[node["type"]]
        if read in ("position", "normal", "view_direction"):
            geo = self.single_node("geometry", "ShaderNodeNewGeometry")
            if read == "position":
                if space == "world":
                    return self.split(geo.outputs["Position"], width)
                coords = self.single_node("texcoord", "ShaderNodeTexCoord")
                return self.split(coords.outputs["Object"], width)
            if read == "normal":
                vec = geo.outputs["Normal"]
                if space != "world":
                    vec = self.node("ShaderNodeVectorTransform", [("Vector", vec)], vector_type="NORMAL",
                                    convert_from="WORLD", convert_to="OBJECT").outputs[0]
                return self.split(vec, width)
            incoming = self.split(geo.outputs["Incoming"])
            view = [self.mul(c, -1.0) for c in incoming]
            if space != "world":
                vec = self.node("ShaderNodeVectorTransform", [("Vector", self.vector(view))], vector_type="VECTOR",
                                convert_from="WORLD", convert_to="OBJECT").outputs[0]
                return self.split(vec, width)
            return view[:width]
        if read in ("tangent", "bitangent"):
            tangent = self.single_node("tangent", "ShaderNodeTangent", direction_type="UV_MAP").outputs["Tangent"]
            if read == "bitangent":
                normal = self.single_node("geometry", "ShaderNodeNewGeometry").outputs["Normal"]
                tangent = self.node("ShaderNodeVectorMath", [(0, normal), (1, tangent)], operation="CROSS_PRODUCT").outputs[0]
            if space != "world":
                tangent = self.node("ShaderNodeVectorTransform", [("Vector", tangent)], vector_type="VECTOR",
                                    convert_from="WORLD", convert_to="OBJECT").outputs[0]
            return self.split(tangent, width)
        if read == "texcoord":
            if node.get("index", 0) != 0:
                self.warn(f"texcoord index {node['index']} reads the active UV map")
            uv = self.single_node("uv", "ShaderNodeTexCoord").outputs["UV"]
            return self.widen(self.split(uv, 2), width) if width != 3 else self.split(uv, 2) + [0.0]
        if read == "geomcolor":
            attr = self.single_node("color", "ShaderNodeVertexColor")
            comps = self.split(attr.outputs["Color"])
            return self.widen(comps + [attr.outputs["Alpha"]], width) if width == 4 else self.widen(comps, width)
        if read == "geomprop":
            name = node.get("name", "")
            attr = self.node("ShaderNodeAttribute", attribute_name=name)
            attr.hide = False
            if width == 1:
                return [attr.outputs["Fac"]]
            return self.widen(self.split(attr.outputs["Vector"]), width)
        if read in ("time", "frame"):
            n = self.node("ShaderNodeValue")
            n.hide = False
            n.label = read
            driver = n.outputs[0].driver_add("default_value").driver
            driver.expression = "frame" if read == "frame" else "frame / 24.0"
            return [n.outputs[0]]
        self.warn(f"context read {read} is not supported")
        return [0.0] * width

    def single_node(self, key, kind, **props):
        if key not in self.single:
            n = self.node(kind, **props)
            n.hide = False
            self.single[key] = n
        return self.single[key]

    def sample(self, node):
        desc = self.doc["textures"][node["texture"]]
        width = WIDTH[node["type"]]
        uv = self.value(node["uv"])
        img = self.node("ShaderNodeTexImage", [("Vector", self.vector(uv[:2]))])
        img.hide = False
        path = os.path.join(self.base_dir, desc["file"])
        try:
            image = bpy.data.images.load(path, check_existing=True)
            # Shadewire's ops convert from the authored colorspace: the texels must stay raw.
            image.colorspace_settings.name = "Non-Color"
            img.image = image
        except RuntimeError:
            self.warn(f"texture not found: {path}")
        modes = {desc["address_u"], desc["address_v"]}
        img.extension = (
            "REPEAT" if "periodic" in modes else "MIRROR" if "mirror" in modes else "CLIP" if "constant" in modes else "EXTEND"
        )
        img.interpolation = {"closest": "Closest", "cubic": "Cubic"}.get(desc["filter"], "Linear")
        comps = self.split(img.outputs["Color"]) + [img.outputs["Alpha"]]
        out = comps[:width] if width > 1 else comps[:1]
        if "constant" in modes:
            default = self.widen(self.constant(node), width)
            inside = 1.0
            for c in uv[:2]:
                inside = self.mul(inside, self.mul(self.math("GREATER_THAN", c, -1e-6), self.math("LESS_THAN", c, 1.0 + 1e-6)))
            out = [self.lerp(d, o, inside) for d, o in zip(default, out)]
        return out

    # ------------------------------------------------------------------ ops

    def op(self, node):
        name = node["op"]
        ty = node["type"]
        width = WIDTH[ty]
        args = [self.value(a) for a in node.get("args", [])]

        def each(fn, *vals):
            vals = [self.widen(v, width) for v in vals]
            return [fn(*[v[k] for v in vals]) for k in range(width)]

        simple = {
            "add": "ADD", "subtract": "SUBTRACT", "multiply": "MULTIPLY", "divide": "DIVIDE",
            "modulo": "FLOORED_MODULO", "absval": "ABSOLUTE", "sign": "SIGN", "floor": "FLOOR", "ceil": "CEIL",
            "round": "ROUND", "fract": "FRACT", "power": "POWER", "sqrt": "SQRT", "exp": "EXPONENT",
            "min": "MINIMUM", "max": "MAXIMUM", "sin": "SINE", "cos": "COSINE", "tan": "TANGENT",
            "asin": "ARCSINE", "acos": "ARCCOSINE", "atan2": "ARCTAN2",
        }
        if name in simple:
            if name == "add":
                return each(self.add, *args)
            if name == "subtract":
                return each(self.sub, *args)
            if name == "multiply":
                return each(self.mul, *args)
            return each(lambda *a: self.math(simple[name], *a), *args)
        if name == "ln":
            return each(lambda a: self.math("LOGARITHM", a, math.e), *args)
        if name == "invert":
            return each(lambda a, amount: self.sub(amount, a), *args)
        if name == "safe_power":
            return each(lambda a, b: self.mul(self.math("SIGN", a), self.math("POWER", self.math("ABSOLUTE", a), b)), *args)
        if name == "clamp":
            return each(lambda a, lo, hi: self.math("MINIMUM", self.math("MAXIMUM", a, lo), hi), *args)
        if name == "mix":
            return each(lambda fg, bg, t: self.lerp(bg, fg, t), *args)
        if name == "smoothstep":
            return each(lambda a, lo, hi: self.map_range(a, lo, hi, 0.0, 1.0, "SMOOTHSTEP"), *args)
        if name == "remap":
            return each(lambda a, il, ih, ol, oh: self.map_range(a, il, ih, ol, oh, "LINEAR"), *args)
        if name == "greater":
            return [self.math("GREATER_THAN", args[0][0], args[1][0])]
        if name == "greater_eq":
            return [self.sub(1.0, self.math("LESS_THAN", args[0][0], args[1][0]))]
        if name == "equal":
            return [self.equal(args[0], args[1])]
        if name in ("if_greater", "if_greater_eq", "if_equal"):
            if name == "if_greater":
                cond = self.math("GREATER_THAN", args[0][0], args[1][0])
            elif name == "if_greater_eq":
                cond = self.sub(1.0, self.math("LESS_THAN", args[0][0], args[1][0]))
            else:
                cond = self.equal(args[0], args[1])
            return each(lambda a, b: self.lerp(b, a, cond), args[2], args[3])
        if name == "switch":
            which = args[-1][0]
            result = self.widen(args[-2], width) if len(args) > 1 else [0.0] * width
            for k in range(len(args) - 2, 0, -1):
                cond = self.math("LESS_THAN", which, float(k))
                result = [self.lerp(r, c, cond) for r, c in zip(result, self.widen(args[k - 1], width))]
            return result
        if name == "and":
            return [self.mul(args[0][0], args[1][0])]
        if name == "or":
            return [self.math("MAXIMUM", args[0][0], args[1][0])]
        if name == "not":
            return [self.sub(1.0, args[0][0])]
        if name == "extract":
            return [args[0][node["index"]]]
        if name == "combine":
            return [c for a in args for c in a][:width]
        if name == "convert":
            src = args[0]
            if len(src) == 1:
                return src * width
            return src[:width] if len(src) >= width else src + [1.0] * (width - len(src))
        if name == "luminance":
            rgb, coeffs = args[0], args[1]
            lum = self.add(self.add(self.mul(rgb[0], coeffs[0]), self.mul(rgb[1], coeffs[1])), self.mul(rgb[2], coeffs[2]))
            return [lum] * 3 + rgb[3:4]
        if name in ("rgb_to_hsv", "hsv_to_rgb"):
            src = args[0]
            if name == "rgb_to_hsv":
                sep = self.node("ShaderNodeSeparateColor", [(0, self.vector(src[:3]))], mode="HSV")
                return [sep.outputs[0], sep.outputs[1], sep.outputs[2]] + src[3:4]
            comb = self.node("ShaderNodeCombineColor", [(k, c) for k, c in enumerate(src[:3])], mode="HSV")
            return self.split(comb.outputs[0]) + src[3:4]
        if name == "normalize":
            return self.split(self.vmath("NORMALIZE", args[0]).outputs[0], width)
        if name == "magnitude":
            return [self.vmath("LENGTH", args[0]).outputs["Value"]]
        if name == "distance":
            return [self.vmath("DISTANCE", args[0], args[1]).outputs["Value"]]
        if name == "dot_product":
            return [self.vmath("DOT_PRODUCT", args[0], args[1]).outputs["Value"]]
        if name == "cross_product":
            return self.split(self.vmath("CROSS_PRODUCT", args[0], args[1]).outputs[0], width)
        if name == "rotate2d":
            v, amount = args[0], args[1][0]
            radians = self.mul(amount, math.pi / 180.0)
            s, c = self.math("SINE", radians), self.math("COSINE", radians)
            return [self.add(self.mul(c, v[0]), self.mul(s, v[1])), self.sub(self.mul(c, v[1]), self.mul(s, v[0]))]
        if name == "transform":
            space = {"model": "OBJECT", "object": "OBJECT", "world": "WORLD"}
            src, dst = space[node["from"]], space[node["to"]]
            if src == dst:
                return args[0][:width]
            n = self.node("ShaderNodeVectorTransform", [("Vector", self.vector(args[0]))],
                          vector_type=node["transform_kind"].upper(), convert_from=src, convert_to=dst)
            return self.split(n.outputs[0], width)
        if name == "normal_map":
            return self.normal_map(*args)
        if name in ("noise2d", "noise3d", "fractal3d", "cell_noise2d", "cell_noise3d", "worley_noise2d", "worley_noise3d"):
            return self.noise(name, args, width)
        if name in ("dpdx", "dpdy"):
            return [0.0] * width
        self.warn(f"op {name} is not supported; it reads as zero")
        return [0.0] * width

    def map_range(self, a, lo, hi, out_lo, out_hi, mode):
        n = self.node("ShaderNodeMapRange", [(0, a), (1, lo), (2, hi), (3, out_lo), (4, out_hi)],
                      interpolation_type=mode, clamp=(mode == "SMOOTHSTEP"))
        return n.outputs[0]

    def equal(self, a, b):
        result = 1.0
        for x, y in zip(self.widen(a, max(len(a), len(b))), self.widen(b, max(len(a), len(b)))):
            result = self.mul(result, self.math("COMPARE", x, y, 0.0))
        return result

    def normal_map(self, value, scale, normal, tangent, bitangent):
        """MaterialX normalmap: decode, scale x and y, bring to the frame, normalize (a zero texel is flat)."""
        scale = self.widen(scale, 2)
        v = [self.math("MULTIPLY_ADD", c, 2.0, -1.0) for c in value[:3]]
        out = []
        for k in range(3):
            term = self.mul(self.mul(tangent[k], v[0]), scale[0])
            term = self.add(term, self.mul(self.mul(bitangent[k], v[1]), scale[1]))
            out.append(self.add(term, self.mul(normal[k], v[2])))
        return self.split(self.vmath("NORMALIZE", out).outputs[0])

    def noise(self, name, args, width):
        """Blender's noises in place of MaterialX's: the same kind of pattern, not the same values."""
        self.warn(f"{name} uses Blender's noise (similar, not identical)")
        if name in ("noise2d", "noise3d"):
            amplitude, pivot, coord = args
            tex = self.node("ShaderNodeTexNoise", [("Vector", self.vector(coord)), ("Scale", 1.0), ("Detail", 0.0)],
                            noise_dimensions="2D" if name == "noise2d" else "3D")
            raw = [tex.outputs["Fac"]] if width == 1 else self.split(tex.outputs["Color"], width)
            amp, piv = self.widen(amplitude, width), self.widen(pivot, width)
            return [self.math("MULTIPLY_ADD", self.math("MULTIPLY_ADD", r, 2.0, -1.0), a, p) for r, a, p in zip(raw, amp, piv)]
        if name == "fractal3d":
            amplitude, octaves, lacunarity, diminish, coord = args
            tex = self.node("ShaderNodeTexNoise", [("Vector", self.vector(coord)), ("Scale", 1.0),
                                                   ("Detail", self.sub(octaves[0], 1.0)), ("Roughness", diminish[0]),
                                                   ("Lacunarity", lacunarity[0])], noise_dimensions="3D")
            raw = [tex.outputs["Fac"]] if width == 1 else self.split(tex.outputs["Color"], width)
            amp = self.widen(amplitude, width)
            return [self.mul(self.math("MULTIPLY_ADD", r, 2.0, -1.0), a) for r, a in zip(raw, amp)]
        if name in ("cell_noise2d", "cell_noise3d"):
            coord = [self.math("FLOOR", c) for c in args[0]]
            tex = self.node("ShaderNodeTexWhiteNoise", [("Vector", self.vector(coord))],
                            noise_dimensions="2D" if name == "cell_noise2d" else "3D")
            return [tex.outputs["Value"]] if width == 1 else self.split(tex.outputs["Color"], width)
        coord, jitter = args[0], args[1]
        tex = self.node("ShaderNodeTexVoronoi", [("Vector", self.vector(coord)), ("Randomness", jitter[0])],
                        voronoi_dimensions="2D" if name == "worley_noise2d" else "3D", feature="F1")
        if width == 1:
            return [tex.outputs["Distance"]]
        second = self.node("ShaderNodeTexVoronoi", [("Vector", self.vector(coord)), ("Randomness", jitter[0])],
                           voronoi_dimensions="2D" if name == "worley_noise2d" else "3D", feature="F2")
        return [tex.outputs["Distance"], second.outputs["Distance"], second.outputs["Distance"]][:width]

    # ------------------------------------------------------------------ the DAG

    def build_values(self):
        for node in self.doc["nodes"]:
            kind = node["kind"]
            width = WIDTH.get(node["type"], 1)
            if kind == "const":
                vals = self.widen(self.constant(node), width)
            elif kind == "uniform":
                vals = self.uniform(node)
            elif kind == "context":
                vals = self.context(node)
            elif kind == "sample":
                vals = self.sample(node)
            else:
                vals = self.op(node)
            self.values.append(vals)

    def field(self, name, width=None):
        for f in self.doc["fields"]:
            if f["name"] == name:
                vals = self.values[f["node"]]
                return self.widen(vals, width) if width else vals
        return None

    # ------------------------------------------------------------------ closures

    def black_shader(self):
        if self.black is None:
            self.black = self.node("ShaderNodeEmission", [("Strength", 0.0)])
            self.black.hide = False
        return self.black.outputs[0]

    def scaled(self, shader, weight):
        """`shader` times a scalar weight, through a Mix Shader with black."""
        if weight == 1.0:
            return shader
        mix = self.node("ShaderNodeMixShader", [(0, weight), (1, self.black_shader()), (2, shader)])
        mix.hide = False
        return mix.outputs[0]

    def luminance(self, comps):
        comps = self.widen(comps, 3)
        return self.add(self.add(self.mul(comps[0], 0.2126), self.mul(comps[1], 0.7152)), self.mul(comps[2], 0.0722))

    def lobe(self, node, weight):
        """A lobe's shader with `weight` (three components) applied, and its weighted albedo."""
        kind = node["kind"]
        inputs = node["inputs"]

        def get(key, width, fallback):
            if key not in inputs:
                return self.widen(fallback, width)
            return self.field(inputs[key], width)

        normal = self.vector(get("normal", 3, [0.0, 0.0, 0.0])) if "normal" in inputs else None
        tangent = self.vector(get("tangent", 3, [0.0, 0.0, 0.0])) if "tangent" in inputs else None
        lobe_weight = get("weight", 1, [1.0])[0]
        w = [self.mul(c, lobe_weight) for c in weight]

        def tinted(color):
            return [self.mul(c, x) for c, x in zip(self.widen(color, 3), w)]

        def roughness(alpha):
            """MaterialX's GGX alpha (vector2) as Blender's roughness, its square root."""
            a = self.mul(self.add(alpha[0], alpha[1]), 0.5)
            return self.math("SQRT", self.math("MAXIMUM", a, 0.0))

        def shader(kind_name, inputs_list, **props):
            n = self.node(kind_name, inputs_list, **props)
            n.hide = False
            if normal is not None and "Normal" in n.inputs:
                self.set_input(n.inputs["Normal"], normal)
            if tangent is not None and "Tangent" in n.inputs:
                self.set_input(n.inputs["Tangent"], tangent)
            return n.outputs[0]

        if kind in ("oren_nayar_diffuse_bsdf", "burley_diffuse_bsdf"):
            color = tinted(get("color", 3, [1.0]))
            s = shader("ShaderNodeBsdfDiffuse", [("Color", self.vector(color)), ("Roughness", get("roughness", 1, [0.0])[0])])
            return s, color
        if kind == "translucent_bsdf":
            color = tinted(get("color", 3, [1.0]))
            return shader("ShaderNodeBsdfTranslucent", [("Color", self.vector(color))]), color
        if kind == "subsurface_bsdf":
            color = tinted(get("color", 3, [1.0]))
            s = shader("ShaderNodeSubsurfaceScattering", [("Color", self.vector(color)), ("Scale", 1.0),
                                                          ("Radius", self.vector(get("radius", 3, [1.0]))),
                                                          ("Anisotropy", get("anisotropy", 1, [0.0])[0])],
                       falloff="RANDOM_WALK")
            return s, color
        if kind == "sheen_bsdf":
            color = tinted(get("color", 3, [1.0]))
            mode = node.get("settings", {}).get("mode", "conty_kulla")
            s = shader("ShaderNodeBsdfSheen", [("Color", self.vector(color)), ("Roughness", get("roughness", 1, [0.3])[0])],
                       distribution="MICROFIBER" if mode == "zeltner" else "ASHIKHMIN")
            return s, [self.mul(c, 0.2) for c in color]
        if kind == "dielectric_bsdf":
            mode = node.get("settings", {}).get("scatter_mode", "R")
            ior = get("ior", 1, [1.5])[0]
            rough = roughness(get("roughness", 2, [0.0]))
            fresnel = self.node("ShaderNodeFresnel", [("IOR", ior)])
            if normal is not None:
                self.set_input(fresnel.inputs["Normal"], normal)
            f = fresnel.outputs[0]
            tint = tinted(get("tint", 3, [1.0]))
            reflected = [self.mul(c, f) for c in tint]
            if mode == "R":
                s = shader("ShaderNodeBsdfGlossy", [("Color", self.vector(reflected)), ("Roughness", rough)], distribution="GGX")
            elif mode == "T":
                passed = [self.mul(c, self.sub(1.0, f)) for c in tint]
                s = shader("ShaderNodeBsdfRefraction", [("Color", self.vector(passed)), ("Roughness", rough), ("IOR", ior)],
                           distribution="GGX")
            else:
                s = shader("ShaderNodeBsdfGlass", [("Color", self.vector(tint)), ("Roughness", rough), ("IOR", ior)],
                           distribution="GGX")
            return s, reflected
        if kind == "conductor_bsdf":
            s = shader("ShaderNodeBsdfMetallic", [("IOR", self.vector(get("ior", 3, [0.18, 0.42, 1.37]))),
                                                  ("Extinction", self.vector(get("extinction", 3, [3.42, 2.35, 1.77]))),
                                                  ("Roughness", roughness(get("roughness", 2, [0.0])))],
                       fresnel_type="PHYSICAL_CONDUCTOR", distribution="GGX")
            return self.scaled(s, self.math("MINIMUM", self.luminance(w), 1.0) if w != [1.0] * 3 else 1.0), w
        if kind == "generalized_schlick_bsdf":
            mode = node.get("settings", {}).get("scatter_mode", "R")
            color0 = get("color0", 3, [1.0])
            rough = roughness(get("roughness", 2, [0.0]))
            if mode == "R":
                s = shader("ShaderNodeBsdfMetallic", [("Base Color", self.vector(color0)),
                                                      ("Edge Tint", self.vector(get("color82", 3, [1.0]))),
                                                      ("Roughness", rough)], fresnel_type="F82", distribution="GGX")
            else:
                f0 = self.math("MINIMUM", self.luminance(color0), 0.99)
                root = self.math("SQRT", f0)
                ior = self.math("DIVIDE", self.add(1.0, root), self.sub(1.0, root))
                s = shader("ShaderNodeBsdfGlass", [("Color", self.vector(get("color90", 3, [1.0]))), ("Roughness", rough),
                                                   ("IOR", ior)], distribution="GGX")
            albedo = [self.mul(c, x) for c, x in zip(color0, w)]
            return self.scaled(s, self.math("MINIMUM", self.luminance(w), 1.0) if w != [1.0] * 3 else 1.0), albedo
        if kind in ("anisotropic_vdf", "absorption_vdf"):
            return None, [0.0, 0.0, 0.0]
        self.warn(f"lobe {kind} is not supported; it shades as nothing")
        return None, [0.0, 0.0, 0.0]

    def bsdf(self, index, weight, out):
        """Appends the shaders of closure node `index` under `weight` to `out`; returns their albedo."""
        node = self.doc["closure"]["nodes"][index]
        op = node["op"]
        if op == "lobe":
            s, albedo = self.lobe(node, weight)
            if s is not None:
                out.append(s)
            return albedo
        if op == "layer":
            top = self.bsdf(node["top"], weight, out)
            below = [self.math("MAXIMUM", self.sub(w, a), 0.0) for w, a in zip(weight, top)]
            base = self.bsdf(node["base"], below, out)
            return [self.add(t, b) for t, b in zip(top, base)]
        if op == "mix":
            m = self.field(node["mix"], 1)[0]
            m = self.math("MINIMUM", self.math("MAXIMUM", m, 0.0), 1.0)
            fg = self.bsdf(node["fg"], [self.mul(w, m) for w in weight], out)
            bg = self.bsdf(node["bg"], [self.mul(w, self.sub(1.0, m)) for w in weight], out)
            return [self.add(a, b) for a, b in zip(fg, bg)]
        if op == "add":
            a = self.bsdf(node["in1"], weight, out)
            b = self.bsdf(node["in2"], weight, out)
            return [self.add(x, y) for x, y in zip(a, b)]
        if op == "multiply":
            scale = self.field(node["in2"], 3)
            return self.bsdf(node["in1"], [self.mul(w, s) for w, s in zip(weight, scale)], out)
        return [0.0, 0.0, 0.0]

    def edf(self, index):
        """The emission of closure node `index`, three components."""
        node = self.doc["closure"]["nodes"][index]
        op = node["op"]
        if op == "lobe":
            inputs = node["inputs"]
            if node["kind"] == "uniform_edf":
                return self.field(inputs["color"], 3) if "color" in inputs else [1.0] * 3
            if node["kind"] == "generalized_schlick_edf":
                base = self.edf(node["closures"]["base"]) if "base" in node.get("closures", {}) else [0.0] * 3
                facing = self.single_node("layer_weight", "ShaderNodeLayerWeight").outputs["Facing"]
                exponent = self.field(inputs["exponent"], 1)[0] if "exponent" in inputs else 5.0
                t = self.math("POWER", facing, exponent)
                c0 = self.field(inputs["color0"], 3) if "color0" in inputs else [1.0] * 3
                c90 = self.field(inputs["color90"], 3) if "color90" in inputs else [1.0] * 3
                return [self.mul(b, self.lerp(a, z, t)) for b, a, z in zip(base, c0, c90)]
            self.warn(f"emitter {node['kind']} is not supported")
            return [0.0] * 3
        if op == "mix":
            m = self.field(node["mix"], 1)[0]
            return [self.lerp(b, f, m) for f, b in zip(self.edf(node["fg"]), self.edf(node["bg"]))]
        if op in ("add", "layer"):
            a = self.edf(node["in1"] if op == "add" else node["top"])
            b = self.edf(node["in2"] if op == "add" else node["base"])
            return [self.add(x, y) for x, y in zip(a, b)]
        if op == "multiply":
            scale = self.field(node["in2"], 3)
            return [self.mul(e, s) for e, s in zip(self.edf(node["in1"]), scale)]
        return [0.0] * 3

    def sum_shaders(self, shaders):
        if not shaders:
            return None
        total = shaders[0]
        for s in shaders[1:]:
            add = self.node("ShaderNodeAddShader", [(0, total), (1, s)])
            add.hide = False
            total = add.outputs[0]
        return total

    def build_surface(self):
        model = self.doc["model"]
        shaders = []
        opacity = 1.0
        if model == "surface" and self.doc.get("closure"):
            closure = self.doc["closure"]
            if closure.get("bsdf") is not None:
                self.bsdf(closure["bsdf"], [1.0, 1.0, 1.0], shaders)
            if closure.get("edf") is not None:
                emission = self.edf(closure["edf"])
                if any(c != 0.0 for c in emission):
                    e = self.node("ShaderNodeEmission", [("Color", self.vector(emission)), ("Strength", 1.0)])
                    e.hide = False
                    shaders.append(e.outputs[0])
            if closure.get("opacity"):
                opacity = self.field(closure["opacity"], 1)[0]
        elif model == "surface_unlit":
            color = [self.mul(e, c) for e, c in zip(self.field("emission", 3), self.field("emission_color", 3))]
            e = self.node("ShaderNodeEmission", [("Color", self.vector(color)), ("Strength", 1.0)])
            e.hide = False
            shaders.append(e.outputs[0])
            opacity = self.field("opacity", 1)[0]
        else:
            self.warn(f"model {model} is not supported")
        surface = self.sum_shaders(shaders) or self.black_shader()
        if opacity != 1.0:
            clear = self.node("ShaderNodeBsdfTransparent")
            clear.hide = False
            mix = self.node("ShaderNodeMixShader", [(0, opacity), (1, clear.outputs[0]), (2, surface)])
            mix.hide = False
            surface = mix.outputs[0]
        out = self.node("ShaderNodeOutputMaterial", [("Surface", surface)])
        out.hide = False

    def build(self):
        self.build_values()
        self.build_surface()
        return self.warnings


def fold(op, a):
    """The value of Math `op` over constants, or None to leave it to a node."""
    try:
        if op == "ADD":
            return a[0] + a[1]
        if op == "SUBTRACT":
            return a[0] - a[1]
        if op == "MULTIPLY":
            return a[0] * a[1]
        if op == "DIVIDE":
            return a[0] / a[1] if a[1] != 0.0 else 0.0
        if op == "MULTIPLY_ADD":
            return a[0] * a[1] + a[2]
        if op == "MINIMUM":
            return min(a[0], a[1])
        if op == "MAXIMUM":
            return max(a[0], a[1])
        if op == "SQRT":
            return math.sqrt(a[0]) if a[0] > 0.0 else 0.0
        if op == "POWER":
            return a[0] ** a[1] if a[0] >= 0.0 else None
        if op == "SINE":
            return math.sin(a[0])
        if op == "COSINE":
            return math.cos(a[0])
        if op == "ABSOLUTE":
            return abs(a[0])
        if op == "SIGN":
            return (a[0] > 0) - (a[0] < 0)
        if op == "FLOOR":
            return math.floor(a[0])
        if op == "GREATER_THAN":
            return 1.0 if a[0] > a[1] else 0.0
        if op == "LESS_THAN":
            return 1.0 if a[0] < a[1] else 0.0
        if op == "COMPARE":
            return 1.0 if abs(a[0] - a[1]) <= a[2] else 0.0
    except (OverflowError, ValueError):
        return None
    return None


def build_material(material, doc, base_dir):
    """Replaces `material`'s node tree with the IR document `doc`; returns the warnings."""
    return Builder(material, doc, base_dir).build()
