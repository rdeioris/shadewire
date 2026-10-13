"""Builds a Cycles node tree from a Shadewire IR document (`shadewire ir --json`).

Values are lists of components; a component is a Python float (a constant, folded on the spot) or a node
output socket. Ops work component by component with Math nodes (vector ops use Vector Math), so every
Shadewire op has an exact node form except the noises, which use Blender's own noise textures.

The closure tree becomes Cycles shaders: each lobe's weight (the product of the mixes, multiplies and layers
above it) goes into the lobe's color when the lobe scales by one, else through a Mix Shader with black. A
`layer` passes its base what the top does not reflect (one minus the top's albedo), as MaterialX's own
GLSL does; the lobes are summed with Add Shader.

Every IR name is looked up in a table of handlers, one table per kind of name: `NODE_KINDS` (DAG node kinds),
`CONTEXT_READS`, `OPS`, `LOBES`, `CLOSURE_OPS` (layer, mix, add, multiply over BSDFs), `EMISSION_OPS` (the
same over EDFs), `EMITTERS` and `MODELS`. A name a table lacks is warned about and reads as zero.
"""

import math
import os

import bpy

WIDTH = {"float": 1, "integer": 1, "boolean": 1, "vector2": 2, "vector3": 3, "color3": 3, "vector4": 4, "color4": 4}
COLUMN = 220
ROW = 160
SPACES = {"model": "OBJECT", "object": "OBJECT", "world": "WORLD"}
BLACK = [0.0, 0.0, 0.0]
WHITE = [1.0, 1.0, 1.0]


class Lobe:
    """A lobe's inputs as the handlers read them: `get` reads an input (or its fallback) at a width, `w` is
    the weight (three components) with the lobe's own `weight` input folded in."""

    def __init__(self, builder, node, weight):
        self.builder = builder
        self.node = node
        self.inputs = node["inputs"]
        self.settings = node.get("settings", {})
        self.normal = builder.vector(self.get("normal", 3, BLACK)) if "normal" in self.inputs else None
        self.tangent = builder.vector(self.get("tangent", 3, BLACK)) if "tangent" in self.inputs else None
        own = self.get("weight", 1, [1.0])[0]
        self.w = [builder.mul(c, own) for c in weight]

    def get(self, key, width, fallback):
        if key not in self.inputs:
            return self.builder.widen(fallback, width)
        return self.builder.field(self.inputs[key], width)

    def tinted(self, color):
        """`color` times the weight: the weight of a lobe whose response scales by its color."""
        return [self.builder.mul(c, x) for c, x in zip(self.builder.widen(color, 3), self.w)]

    def roughness(self, key="roughness"):
        """MaterialX's GGX alpha (vector2) as Blender's roughness, its square root."""
        b = self.builder
        alpha = self.get(key, 2, [0.0])
        return b.math("SQRT", b.math("MAXIMUM", b.mul(b.add(alpha[0], alpha[1]), 0.5), 0.0))

    def shader(self, kind, inputs, **props):
        """A shader node with the lobe's normal and tangent linked; its output."""
        b = self.builder
        n = b.node(kind, inputs, **props)
        n.hide = False
        if self.normal is not None and "Normal" in n.inputs:
            b.set_input(n.inputs["Normal"], self.normal)
        if self.tangent is not None and "Tangent" in n.inputs:
            b.set_input(n.inputs["Tangent"], self.tangent)
        return n.outputs[0]

    def scaled(self, shader):
        """`shader` under the weight's luminance, for lobes without a color to tint."""
        b = self.builder
        if self.w == WHITE:
            return shader
        return b.scaled(shader, b.math("MINIMUM", b.luminance(self.w), 1.0))


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
        n.hide = n.bl_idname in ("ShaderNodeMath", "ShaderNodeSeparateXYZ", "ShaderNodeCombineXYZ")

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
            socket.default_value = list(value)[:n] + [1.0] * max(0, n - len(value))
            return 0
        self.tree.links.new(value, socket)
        return self.depth.get(value.node.name, 0)

    def single_node(self, key, kind, **props):
        """One shared node per key (the Geometry node, the UV map...)."""
        if key not in self.single:
            n = self.node(kind, **props)
            n.hide = False
            self.single[key] = n
        return self.single[key]

    # ------------------------------------------------------------------ component math

    def math(self, op, *args, **props):
        if all(isinstance(a, (int, float)) for a in args):
            folded = fold(op, [float(a) for a in args])
            if folded is not None:
                return folded
        return self.node("ShaderNodeMath", list(enumerate(args)), operation=op, **props).outputs[0]

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

    def luminance(self, comps):
        comps = self.widen(comps, 3)
        return self.add(self.add(self.mul(comps[0], 0.2126), self.mul(comps[1], 0.7152)), self.mul(comps[2], 0.0722))

    def split(self, socket, width=3):
        """The components of a vector or color socket."""
        key = (socket.node.name, socket.identifier)
        if key not in self.split_cache:
            n = self.node("ShaderNodeSeparateXYZ", [("Vector", socket)])
            self.split_cache[key] = list(n.outputs[:3])
        return self.split_cache[key][:width]

    def vector(self, comps):
        """A vector socket (or a constant list) holding up to three components."""
        comps = list(comps) + [0.0] * (3 - len(comps))
        if all(isinstance(c, (int, float)) for c in comps[:3]):
            return [float(c) for c in comps[:3]]
        return self.node("ShaderNodeCombineXYZ", list(enumerate(comps[:3]))).outputs[0]

    def vmath(self, op, *vectors):
        """Vector Math over three-component values; returns the node."""
        return self.node("ShaderNodeVectorMath", [(k, self.vector(v)) for k, v in enumerate(vectors)], operation=op)

    def transform(self, vector_socket, kind, src="WORLD", dst="OBJECT"):
        n = self.node("ShaderNodeVectorTransform", [("Vector", vector_socket)], vector_type=kind,
                      convert_from=src, convert_to=dst)
        return n.outputs[0]

    def map_range(self, a, lo, hi, out_lo, out_hi, mode):
        n = self.node("ShaderNodeMapRange", [(0, a), (1, lo), (2, hi), (3, out_lo), (4, out_hi)],
                      interpolation_type=mode, clamp=(mode == "SMOOTHSTEP"))
        return n.outputs[0]

    def equal(self, a, b):
        n = max(len(a), len(b))
        result = 1.0
        for x, y in zip(self.widen(a, n), self.widen(b, n)):
            result = self.mul(result, self.math("COMPARE", x, y, 0.0))
        return result

    # ------------------------------------------------------------------ values

    def widen(self, comps, width):
        """A scalar broadcast to `width` components; longer values truncate, shorter ones pad with 1."""
        if len(comps) == 1 and width > 1:
            return comps * width
        return comps[:width] if len(comps) >= width else comps + [1.0] * (width - len(comps))

    def each(self, width, fn, *vals):
        """`fn` over the components of `vals` (scalars broadcast)."""
        vals = [self.widen(v, width) for v in vals]
        return [fn(*[v[k] for v in vals]) for k in range(width)]

    def field(self, name, width=None):
        for f in self.doc["fields"]:
            if f["name"] == name:
                vals = self.values[f["node"]]
                return self.widen(vals, width) if width else vals
        return None

    def build_values(self):
        for node in self.doc["nodes"]:
            width = WIDTH.get(node["type"], 1)
            handler = NODE_KINDS.get(node["kind"])
            if handler is None:
                self.warn(f"node kind {node['kind']} is not supported; it reads as zero")
                self.values.append([0.0] * width)
                continue
            self.values.append(handler(self, node, width))

    # ------------------------------------------------------------------ DAG node kinds

    def const_node(self, node, width):
        return self.widen(constant(node), width)

    def uniform_node(self, node, width):
        """An editable value: a Value node per scalar, an RGB node for colors."""
        vals = constant(node)
        label = node.get("name", "")
        if node["type"] in ("color3", "color4"):
            n = self.node("ShaderNodeRGB")
            n.outputs[0].default_value = vals[:3] + [1.0]
            n.label = label
            n.hide = False
            return self.split(n.outputs[0]) + vals[3:4]
        comps = []
        for k, x in enumerate(vals):
            n = self.node("ShaderNodeValue")
            n.outputs[0].default_value = x
            n.label = label if len(vals) == 1 else f"{label}[{k}]"
            n.hide = False
            comps.append(n.outputs[0])
        return comps

    def context_node(self, node, width):
        handler = CONTEXT_READS.get(node["read"])
        if handler is None:
            self.warn(f"context read {node['read']} is not supported")
            return [0.0] * width
        return handler(self, node, node.get("space", "world"), width)

    def sample_node(self, node, width):
        desc = self.doc["textures"][node["texture"]]
        uv = self.values[node["uv"]]
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
        img.extension = next((ext for mode, ext in EXTENSIONS if mode in modes), "EXTEND")
        img.interpolation = INTERPOLATIONS.get(desc["filter"], "Linear")
        out = (self.split(img.outputs["Color"]) + [img.outputs["Alpha"]])[:max(width, 1)]
        if "constant" in modes:
            default = self.widen(constant(node), width)
            inside = 1.0
            for c in uv[:2]:
                inside = self.mul(inside, self.mul(self.math("GREATER_THAN", c, -1e-6), self.math("LESS_THAN", c, 1.0 + 1e-6)))
            out = [self.lerp(d, o, inside) for d, o in zip(default, out)]
        return out

    def op_node(self, node, width):
        handler = OPS.get(node["op"])
        if handler is None:
            self.warn(f"op {node['op']} is not supported; it reads as zero")
            return [0.0] * width
        args = [self.values[a] for a in node.get("args", [])]
        return handler(self, node, args, width)

    # ------------------------------------------------------------------ context reads

    def read_position(self, node, space, width):
        if space == "world":
            return self.split(self.single_node("geometry", "ShaderNodeNewGeometry").outputs["Position"], width)
        return self.split(self.single_node("texcoord", "ShaderNodeTexCoord").outputs["Object"], width)

    def read_normal(self, node, space, width):
        vec = self.single_node("geometry", "ShaderNodeNewGeometry").outputs["Normal"]
        if space != "world":
            vec = self.transform(vec, "NORMAL")
        return self.split(vec, width)

    def read_view_direction(self, node, space, width):
        """MaterialX's view direction points from the camera to the surface: Blender's Incoming, negated."""
        incoming = self.split(self.single_node("geometry", "ShaderNodeNewGeometry").outputs["Incoming"])
        view = [self.mul(c, -1.0) for c in incoming]
        if space != "world":
            return self.split(self.transform(self.vector(view), "VECTOR"), width)
        return view[:width]

    def uv_tangent(self):
        return self.single_node("tangent", "ShaderNodeTangent", direction_type="UV_MAP").outputs["Tangent"]

    def read_tangent(self, node, space, width):
        tangent = self.uv_tangent()
        return self.split(tangent if space == "world" else self.transform(tangent, "VECTOR"), width)

    def read_bitangent(self, node, space, width):
        normal = self.single_node("geometry", "ShaderNodeNewGeometry").outputs["Normal"]
        bitangent = self.node("ShaderNodeVectorMath", [(0, normal), (1, self.uv_tangent())],
                              operation="CROSS_PRODUCT").outputs[0]
        return self.split(bitangent if space == "world" else self.transform(bitangent, "VECTOR"), width)

    def read_texcoord(self, node, space, width):
        if node.get("index", 0) != 0:
            self.warn(f"texcoord index {node['index']} reads the active UV map")
        uv = self.split(self.single_node("uv", "ShaderNodeTexCoord").outputs["UV"], 2)
        return uv + [0.0] if width == 3 else self.widen(uv, width)

    def read_geomcolor(self, node, space, width):
        attr = self.single_node("color", "ShaderNodeVertexColor")
        return self.widen(self.split(attr.outputs["Color"]) + [attr.outputs["Alpha"]], width)

    def read_geomprop(self, node, space, width):
        attr = self.node("ShaderNodeAttribute", attribute_name=node.get("name", ""))
        attr.hide = False
        if width == 1:
            return [attr.outputs["Fac"]]
        return self.widen(self.split(attr.outputs["Vector"]), width)

    def read_clock(self, node, space, width):
        """`frame`, or `time` in seconds at 24 frames per second, through a driver."""
        n = self.node("ShaderNodeValue")
        n.hide = False
        n.label = node["read"]
        driver = n.outputs[0].driver_add("default_value").driver
        driver.expression = "frame" if node["read"] == "frame" else "frame / 24.0"
        return [n.outputs[0]]

    # ------------------------------------------------------------------ ops

    def op_ln(self, node, args, width):
        return self.each(width, lambda a: self.math("LOGARITHM", a, math.e), *args)

    def op_invert(self, node, args, width):
        return self.each(width, lambda a, amount: self.sub(amount, a), *args)

    def op_safe_power(self, node, args, width):
        def safe(a, b):
            return self.mul(self.math("SIGN", a), self.math("POWER", self.math("ABSOLUTE", a), b))

        return self.each(width, safe, *args)

    def op_clamp(self, node, args, width):
        return self.each(width, lambda a, lo, hi: self.math("MINIMUM", self.math("MAXIMUM", a, lo), hi), *args)

    def op_mix(self, node, args, width):
        return self.each(width, lambda fg, bg, t: self.lerp(bg, fg, t), *args)

    def op_smoothstep(self, node, args, width):
        return self.each(width, lambda a, lo, hi: self.map_range(a, lo, hi, 0.0, 1.0, "SMOOTHSTEP"), *args)

    def op_remap(self, node, args, width):
        return self.each(width, lambda a, il, ih, ol, oh: self.map_range(a, il, ih, ol, oh, "LINEAR"), *args)

    def greater(self, a, b):
        return self.math("GREATER_THAN", a[0], b[0])

    def greater_eq(self, a, b):
        return self.sub(1.0, self.math("LESS_THAN", a[0], b[0]))

    def op_greater(self, node, args, width):
        return [self.greater(*args)]

    def op_greater_eq(self, node, args, width):
        return [self.greater_eq(*args)]

    def op_equal(self, node, args, width):
        return [self.equal(*args)]

    def picking(condition):
        """An `if_*` op: `in1` where `condition(value1, value2)` holds, else `in2`."""

        def op(self, node, args, width):
            cond = condition(self, args[0], args[1])
            return self.each(width, lambda a, b: self.lerp(b, a, cond), args[2], args[3])

        return op

    def op_switch(self, node, args, width):
        """The first `in<k>` whose k exceeds `which`."""
        which = args[-1][0]
        result = self.widen(args[-2], width) if len(args) > 1 else [0.0] * width
        for k in range(len(args) - 2, 0, -1):
            cond = self.math("LESS_THAN", which, float(k))
            result = [self.lerp(r, c, cond) for r, c in zip(result, self.widen(args[k - 1], width))]
        return result

    def op_and(self, node, args, width):
        return [self.mul(args[0][0], args[1][0])]

    def op_or(self, node, args, width):
        return [self.math("MAXIMUM", args[0][0], args[1][0])]

    def op_not(self, node, args, width):
        return [self.sub(1.0, args[0][0])]

    def op_extract(self, node, args, width):
        return [args[0][node["index"]]]

    def op_combine(self, node, args, width):
        return [c for a in args for c in a][:width]

    def op_convert(self, node, args, width):
        return self.widen(args[0], width)

    def op_luminance(self, node, args, width):
        rgb, coeffs = args
        lum = self.add(self.add(self.mul(rgb[0], coeffs[0]), self.mul(rgb[1], coeffs[1])), self.mul(rgb[2], coeffs[2]))
        return [lum] * 3 + rgb[3:4]

    def op_rgb_to_hsv(self, node, args, width):
        src = args[0]
        sep = self.node("ShaderNodeSeparateColor", [(0, self.vector(src[:3]))], mode="HSV")
        return list(sep.outputs[:3]) + src[3:4]

    def op_hsv_to_rgb(self, node, args, width):
        src = args[0]
        comb = self.node("ShaderNodeCombineColor", list(enumerate(src[:3])), mode="HSV")
        return self.split(comb.outputs[0]) + src[3:4]

    def vector_op(operation, output=0, scalar=False):
        """A Vector Math op; `scalar` ones read its Value output."""

        def op(self, node, args, width):
            n = self.vmath(operation, *args)
            return [n.outputs["Value"]] if scalar else self.split(n.outputs[output], width)

        return op

    def op_rotate2d(self, node, args, width):
        v, amount = args[0], args[1][0]
        radians = self.mul(amount, math.pi / 180.0)
        s, c = self.math("SINE", radians), self.math("COSINE", radians)
        return [self.add(self.mul(c, v[0]), self.mul(s, v[1])), self.sub(self.mul(c, v[1]), self.mul(s, v[0]))]

    def op_transform(self, node, args, width):
        src, dst = SPACES[node["from"]], SPACES[node["to"]]
        if src == dst:
            return args[0][:width]
        vec = self.transform(self.vector(args[0]), node["transform_kind"].upper(), src, dst)
        return self.split(vec, width)

    def op_normal_map(self, node, args, width):
        """MaterialX normalmap: decode, scale x and y, bring to the frame, normalize."""
        value, scale, normal, tangent, bitangent = args
        scale = self.widen(scale, 2)
        v = [self.math("MULTIPLY_ADD", c, 2.0, -1.0) for c in value[:3]]
        out = []
        for k in range(3):
            term = self.mul(self.mul(tangent[k], v[0]), scale[0])
            term = self.add(term, self.mul(self.mul(bitangent[k], v[1]), scale[1]))
            out.append(self.add(term, self.mul(normal[k], v[2])))
        return self.split(self.vmath("NORMALIZE", out).outputs[0])

    def op_zero(self, node, args, width):
        """Screen-space derivatives: Cycles shading has none."""
        return [0.0] * width

    def approximate(self, node):
        self.warn(f"{node['op']} uses Blender's noise (similar, not identical)")

    def noise_texture(self, coord, dimensions, width, **inputs):
        """Blender's Noise Texture, signed like MaterialX's Perlin noise (-1..1)."""
        extra = [(key.capitalize(), value) for key, value in inputs.items()]
        tex = self.node("ShaderNodeTexNoise", [("Vector", self.vector(coord)), ("Scale", 1.0), *extra],
                        noise_dimensions=dimensions)
        raw = [tex.outputs["Fac"]] if width == 1 else self.split(tex.outputs["Color"], width)
        return [self.math("MULTIPLY_ADD", r, 2.0, -1.0) for r in raw]

    def perlin(dimensions):
        def op(self, node, args, width):
            self.approximate(node)
            amplitude, pivot, coord = args
            signed = self.noise_texture(coord, dimensions, width, detail=0.0)
            return self.each(width, lambda n, a, p: self.math("MULTIPLY_ADD", n, a, p), signed, amplitude, pivot)

        return op

    def op_fractal3d(self, node, args, width):
        self.approximate(node)
        amplitude, octaves, lacunarity, diminish, coord = args
        signed = self.noise_texture(coord, "3D", width, detail=self.sub(octaves[0], 1.0), roughness=diminish[0],
                                    lacunarity=lacunarity[0])
        return self.each(width, self.mul, signed, amplitude)

    def cell(dimensions):
        def op(self, node, args, width):
            self.approximate(node)
            coord = [self.math("FLOOR", c) for c in args[0]]
            tex = self.node("ShaderNodeTexWhiteNoise", [("Vector", self.vector(coord))], noise_dimensions=dimensions)
            return [tex.outputs["Value"]] if width == 1 else self.split(tex.outputs["Color"], width)

        return op

    def worley(dimensions):
        """F1 distance; vector results take F2 for the other components."""

        def op(self, node, args, width):
            self.approximate(node)
            coord, jitter = args[0], args[1][0]

            def voronoi(feature):
                return self.node("ShaderNodeTexVoronoi", [("Vector", self.vector(coord)), ("Randomness", jitter)],
                                 voronoi_dimensions=dimensions, feature=feature).outputs["Distance"]

            f1 = voronoi("F1")
            if width == 1:
                return [f1]
            f2 = voronoi("F2")
            return [f1, f2, f2][:width]

        return op

    # ------------------------------------------------------------------ lobes

    def lobe_diffuse(self, lobe):
        color = lobe.tinted(lobe.get("color", 3, [1.0]))
        s = lobe.shader("ShaderNodeBsdfDiffuse", [("Color", self.vector(color)), ("Roughness", lobe.get("roughness", 1, [0.0])[0])])
        return s, color

    def lobe_translucent(self, lobe):
        color = lobe.tinted(lobe.get("color", 3, [1.0]))
        return lobe.shader("ShaderNodeBsdfTranslucent", [("Color", self.vector(color))]), color

    def lobe_subsurface(self, lobe):
        color = lobe.tinted(lobe.get("color", 3, [1.0]))
        s = lobe.shader("ShaderNodeSubsurfaceScattering",
                        [("Color", self.vector(color)), ("Scale", 1.0), ("Radius", self.vector(lobe.get("radius", 3, [1.0]))),
                         ("Anisotropy", lobe.get("anisotropy", 1, [0.0])[0])], falloff="RANDOM_WALK")
        return s, color

    def lobe_sheen(self, lobe):
        color = lobe.tinted(lobe.get("color", 3, [1.0]))
        mode = lobe.settings.get("mode", "conty_kulla")
        s = lobe.shader("ShaderNodeBsdfSheen", [("Color", self.vector(color)), ("Roughness", lobe.get("roughness", 1, [0.3])[0])],
                        distribution="MICROFIBER" if mode == "zeltner" else "ASHIKHMIN")
        return s, [self.mul(c, 0.2) for c in color]

    def lobe_dielectric(self, lobe):
        """`R`: Glossy times Fresnel; `T`: Refraction times one minus Fresnel; `RT`: Glass."""
        ior = lobe.get("ior", 1, [1.5])[0]
        rough = lobe.roughness()
        fresnel = self.node("ShaderNodeFresnel", [("IOR", ior)])
        if lobe.normal is not None:
            self.set_input(fresnel.inputs["Normal"], lobe.normal)
        f = fresnel.outputs[0]
        tint = lobe.tinted(lobe.get("tint", 3, [1.0]))
        reflected = [self.mul(c, f) for c in tint]
        mode = lobe.settings.get("scatter_mode", "R")
        if mode == "R":
            s = lobe.shader("ShaderNodeBsdfGlossy", [("Color", self.vector(reflected)), ("Roughness", rough)], distribution="GGX")
        elif mode == "T":
            passed = [self.mul(c, self.sub(1.0, f)) for c in tint]
            s = lobe.shader("ShaderNodeBsdfRefraction", [("Color", self.vector(passed)), ("Roughness", rough), ("IOR", ior)],
                            distribution="GGX")
        else:
            s = lobe.shader("ShaderNodeBsdfGlass", [("Color", self.vector(tint)), ("Roughness", rough), ("IOR", ior)],
                            distribution="GGX")
        return s, reflected

    def lobe_conductor(self, lobe):
        s = lobe.shader("ShaderNodeBsdfMetallic",
                        [("IOR", self.vector(lobe.get("ior", 3, [0.18, 0.42, 1.37]))),
                         ("Extinction", self.vector(lobe.get("extinction", 3, [3.42, 2.35, 1.77]))),
                         ("Roughness", lobe.roughness())], fresnel_type="PHYSICAL_CONDUCTOR", distribution="GGX")
        return lobe.scaled(s), lobe.w

    def lobe_schlick(self, lobe):
        """Reflection through the Metallic BSDF's F82 model; transmission through Glass at the IOR of `color0`."""
        color0 = lobe.get("color0", 3, [1.0])
        rough = lobe.roughness()
        if lobe.settings.get("scatter_mode", "R") == "R":
            s = lobe.shader("ShaderNodeBsdfMetallic",
                            [("Base Color", self.vector(color0)), ("Edge Tint", self.vector(lobe.get("color82", 3, [1.0]))),
                             ("Roughness", rough)], fresnel_type="F82", distribution="GGX")
        else:
            root = self.math("SQRT", self.math("MINIMUM", self.luminance(color0), 0.99))
            ior = self.math("DIVIDE", self.add(1.0, root), self.sub(1.0, root))
            s = lobe.shader("ShaderNodeBsdfGlass", [("Color", self.vector(lobe.get("color90", 3, [1.0]))), ("Roughness", rough),
                                                    ("IOR", ior)], distribution="GGX")
        return lobe.scaled(s), [self.mul(c, x) for c, x in zip(color0, lobe.w)]

    def lobe_volume(self, lobe):
        """Volumes are not translated: the surface above them shades alone."""
        return None, BLACK

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

    def closure(self, index):
        return self.doc["closure"]["nodes"][index]

    def bsdf(self, index, weight, out):
        """Appends the shaders of closure node `index` under `weight` to `out`; returns their albedo."""
        node = self.closure(index)
        return CLOSURE_OPS[node["op"]](self, node, weight, out)

    def bsdf_lobe(self, node, weight, out):
        handler = LOBES.get(node["kind"])
        if handler is None:
            self.warn(f"lobe {node['kind']} is not supported; it shades as nothing")
            return BLACK
        shader, albedo = handler(self, Lobe(self, node, weight))
        if shader is not None:
            out.append(shader)
        return albedo

    def bsdf_layer(self, node, weight, out):
        top = self.bsdf(node["top"], weight, out)
        below = [self.math("MAXIMUM", self.sub(w, a), 0.0) for w, a in zip(weight, top)]
        base = self.bsdf(node["base"], below, out)
        return [self.add(t, b) for t, b in zip(top, base)]

    def mix_weight(self, node):
        m = self.field(node["mix"], 1)[0]
        return self.math("MINIMUM", self.math("MAXIMUM", m, 0.0), 1.0)

    def bsdf_mix(self, node, weight, out):
        m = self.mix_weight(node)
        fg = self.bsdf(node["fg"], [self.mul(w, m) for w in weight], out)
        bg = self.bsdf(node["bg"], [self.mul(w, self.sub(1.0, m)) for w in weight], out)
        return [self.add(a, b) for a, b in zip(fg, bg)]

    def bsdf_add(self, node, weight, out):
        a = self.bsdf(node["in1"], weight, out)
        b = self.bsdf(node["in2"], weight, out)
        return [self.add(x, y) for x, y in zip(a, b)]

    def bsdf_multiply(self, node, weight, out):
        scale = self.field(node["in2"], 3)
        return self.bsdf(node["in1"], [self.mul(w, s) for w, s in zip(weight, scale)], out)

    def emission(self, index):
        """The emission of closure node `index`, three components."""
        node = self.closure(index)
        return EMISSION_OPS[node["op"]](self, node)

    def emission_lobe(self, node):
        handler = EMITTERS.get(node["kind"])
        if handler is None:
            self.warn(f"emitter {node['kind']} is not supported")
            return BLACK
        return handler(self, node)

    def emission_mix(self, node):
        m = self.field(node["mix"], 1)[0]
        return [self.lerp(b, f, m) for f, b in zip(self.emission(node["fg"]), self.emission(node["bg"]))]

    def emission_add(self, node):
        return [self.add(x, y) for x, y in zip(self.emission(node["in1"]), self.emission(node["in2"]))]

    def emission_layer(self, node):
        return [self.add(x, y) for x, y in zip(self.emission(node["top"]), self.emission(node["base"]))]

    def emission_multiply(self, node):
        scale = self.field(node["in2"], 3)
        return [self.mul(e, s) for e, s in zip(self.emission(node["in1"]), scale)]

    def emitter_input(self, node, key, width, fallback):
        inputs = node["inputs"]
        return self.field(inputs[key], width) if key in inputs else self.widen(fallback, width)

    def emitter_uniform(self, node):
        return self.emitter_input(node, "color", 3, WHITE)

    def emitter_schlick(self, node):
        """`base` shaped by mix(color0, color90, facing ^ exponent)."""
        closures = node.get("closures", {})
        base = self.emission(closures["base"]) if "base" in closures else BLACK
        facing = self.single_node("layer_weight", "ShaderNodeLayerWeight").outputs["Facing"]
        t = self.math("POWER", facing, self.emitter_input(node, "exponent", 1, [5.0])[0])
        c0 = self.emitter_input(node, "color0", 3, WHITE)
        c90 = self.emitter_input(node, "color90", 3, WHITE)
        return [self.mul(b, self.lerp(a, z, t)) for b, a, z in zip(base, c0, c90)]

    # ------------------------------------------------------------------ the surface

    def emission_shader(self, color):
        e = self.node("ShaderNodeEmission", [("Color", self.vector(color)), ("Strength", 1.0)])
        e.hide = False
        return e.outputs[0]

    def model_surface(self):
        """The closure tree: its lobes, its emission, its opacity."""
        closure = self.doc.get("closure") or {}
        shaders = []
        if closure.get("bsdf") is not None:
            self.bsdf(closure["bsdf"], WHITE, shaders)
        if closure.get("edf") is not None:
            color = self.emission(closure["edf"])
            if any(c != 0.0 for c in color):
                shaders.append(self.emission_shader(color))
        opacity = self.field(closure["opacity"], 1)[0] if closure.get("opacity") else 1.0
        return shaders, opacity

    def model_unlit(self):
        color = [self.mul(e, c) for e, c in zip(self.field("emission", 3), self.field("emission_color", 3))]
        return [self.emission_shader(color)], self.field("opacity", 1)[0]

    def sum_shaders(self, shaders):
        if not shaders:
            return self.black_shader()
        total = shaders[0]
        for s in shaders[1:]:
            add = self.node("ShaderNodeAddShader", [(0, total), (1, s)])
            add.hide = False
            total = add.outputs[0]
        return total

    def build_surface(self):
        handler = MODELS.get(self.doc["model"])
        if handler is None:
            self.warn(f"model {self.doc['model']} is not supported")
            shaders, opacity = [], 1.0
        else:
            shaders, opacity = handler(self)
        surface = self.sum_shaders(shaders)
        if opacity != 1.0:
            clear = self.node("ShaderNodeBsdfTransparent")
            clear.hide = False
            mix = self.node("ShaderNodeMixShader", [(0, opacity), (1, clear.outputs[0]), (2, surface)])
            mix.hide = False
            surface = mix.outputs[0]
        self.node("ShaderNodeOutputMaterial", [("Surface", surface)]).hide = False

    def build(self):
        self.build_values()
        self.build_surface()
        return self.warnings


def constant(node):
    """A node's `value` as components (booleans as 0 and 1)."""
    v = node.get("value")
    if isinstance(v, bool):
        return [1.0 if v else 0.0]
    if isinstance(v, (int, float)):
        return [float(v)]
    return [float(x) for x in v]


def math_op(operation):
    """An op that is one Math node per component."""

    def op(self, node, args, width):
        return self.each(width, lambda *a: self.math(operation, *a), *args)

    return op


def component_op(method):
    """An op that is a Builder helper per component (`add`, `sub`, `mul` fold their identities)."""

    def op(self, node, args, width):
        return self.each(width, getattr(self, method), *args)

    return op


# The address modes in order of precedence (one mode per image in Blender), then the filters.
EXTENSIONS = (("periodic", "REPEAT"), ("mirror", "MIRROR"), ("constant", "CLIP"))
INTERPOLATIONS = {"closest": "Closest", "cubic": "Cubic", "linear": "Linear"}

NODE_KINDS = {
    "const": Builder.const_node,
    "uniform": Builder.uniform_node,
    "context": Builder.context_node,
    "sample": Builder.sample_node,
    "op": Builder.op_node,
}

CONTEXT_READS = {
    "position": Builder.read_position,
    "normal": Builder.read_normal,
    "view_direction": Builder.read_view_direction,
    "tangent": Builder.read_tangent,
    "bitangent": Builder.read_bitangent,
    "texcoord": Builder.read_texcoord,
    "geomcolor": Builder.read_geomcolor,
    "geomprop": Builder.read_geomprop,
    "time": Builder.read_clock,
    "frame": Builder.read_clock,
}

OPS = {
    "add": component_op("add"),
    "subtract": component_op("sub"),
    "multiply": component_op("mul"),
    "divide": math_op("DIVIDE"),
    "modulo": math_op("FLOORED_MODULO"),
    "absval": math_op("ABSOLUTE"),
    "sign": math_op("SIGN"),
    "floor": math_op("FLOOR"),
    "ceil": math_op("CEIL"),
    "round": math_op("ROUND"),
    "fract": math_op("FRACT"),
    "power": math_op("POWER"),
    "sqrt": math_op("SQRT"),
    "exp": math_op("EXPONENT"),
    "min": math_op("MINIMUM"),
    "max": math_op("MAXIMUM"),
    "sin": math_op("SINE"),
    "cos": math_op("COSINE"),
    "tan": math_op("TANGENT"),
    "asin": math_op("ARCSINE"),
    "acos": math_op("ARCCOSINE"),
    "atan2": math_op("ARCTAN2"),
    "ln": Builder.op_ln,
    "invert": Builder.op_invert,
    "safe_power": Builder.op_safe_power,
    "clamp": Builder.op_clamp,
    "mix": Builder.op_mix,
    "smoothstep": Builder.op_smoothstep,
    "remap": Builder.op_remap,
    "greater": Builder.op_greater,
    "greater_eq": Builder.op_greater_eq,
    "equal": Builder.op_equal,
    "if_greater": Builder.picking(Builder.greater),
    "if_greater_eq": Builder.picking(Builder.greater_eq),
    "if_equal": Builder.picking(Builder.equal),
    "switch": Builder.op_switch,
    "and": Builder.op_and,
    "or": Builder.op_or,
    "not": Builder.op_not,
    "extract": Builder.op_extract,
    "combine": Builder.op_combine,
    "convert": Builder.op_convert,
    "luminance": Builder.op_luminance,
    "rgb_to_hsv": Builder.op_rgb_to_hsv,
    "hsv_to_rgb": Builder.op_hsv_to_rgb,
    "normalize": Builder.vector_op("NORMALIZE"),
    "magnitude": Builder.vector_op("LENGTH", scalar=True),
    "distance": Builder.vector_op("DISTANCE", scalar=True),
    "dot_product": Builder.vector_op("DOT_PRODUCT", scalar=True),
    "cross_product": Builder.vector_op("CROSS_PRODUCT"),
    "rotate2d": Builder.op_rotate2d,
    "transform": Builder.op_transform,
    "normal_map": Builder.op_normal_map,
    "noise2d": Builder.perlin("2D"),
    "noise3d": Builder.perlin("3D"),
    "fractal3d": Builder.op_fractal3d,
    "cell_noise2d": Builder.cell("2D"),
    "cell_noise3d": Builder.cell("3D"),
    "worley_noise2d": Builder.worley("2D"),
    "worley_noise3d": Builder.worley("3D"),
    "dpdx": Builder.op_zero,
    "dpdy": Builder.op_zero,
}

LOBES = {
    "oren_nayar_diffuse_bsdf": Builder.lobe_diffuse,
    "burley_diffuse_bsdf": Builder.lobe_diffuse,
    "translucent_bsdf": Builder.lobe_translucent,
    "subsurface_bsdf": Builder.lobe_subsurface,
    "sheen_bsdf": Builder.lobe_sheen,
    "dielectric_bsdf": Builder.lobe_dielectric,
    "conductor_bsdf": Builder.lobe_conductor,
    "generalized_schlick_bsdf": Builder.lobe_schlick,
    "anisotropic_vdf": Builder.lobe_volume,
    "absorption_vdf": Builder.lobe_volume,
}

CLOSURE_OPS = {
    "lobe": Builder.bsdf_lobe,
    "layer": Builder.bsdf_layer,
    "mix": Builder.bsdf_mix,
    "add": Builder.bsdf_add,
    "multiply": Builder.bsdf_multiply,
}

EMISSION_OPS = {
    "lobe": Builder.emission_lobe,
    "layer": Builder.emission_layer,
    "mix": Builder.emission_mix,
    "add": Builder.emission_add,
    "multiply": Builder.emission_multiply,
}

EMITTERS = {
    "uniform_edf": Builder.emitter_uniform,
    "generalized_schlick_edf": Builder.emitter_schlick,
}

MODELS = {
    "surface": Builder.model_surface,
    "surface_unlit": Builder.model_unlit,
}

# Math over constants, folded in Python instead of making a node; a None result leaves it to a node.
FOLDS = {
    "ADD": lambda a: a[0] + a[1],
    "SUBTRACT": lambda a: a[0] - a[1],
    "MULTIPLY": lambda a: a[0] * a[1],
    "DIVIDE": lambda a: a[0] / a[1] if a[1] != 0.0 else 0.0,
    "MULTIPLY_ADD": lambda a: a[0] * a[1] + a[2],
    "MINIMUM": lambda a: min(a[0], a[1]),
    "MAXIMUM": lambda a: max(a[0], a[1]),
    "SQRT": lambda a: math.sqrt(a[0]) if a[0] > 0.0 else 0.0,
    "POWER": lambda a: a[0] ** a[1] if a[0] >= 0.0 else None,
    "SINE": lambda a: math.sin(a[0]),
    "COSINE": lambda a: math.cos(a[0]),
    "ABSOLUTE": lambda a: abs(a[0]),
    "SIGN": lambda a: float((a[0] > 0) - (a[0] < 0)),
    "FLOOR": lambda a: float(math.floor(a[0])),
    "GREATER_THAN": lambda a: 1.0 if a[0] > a[1] else 0.0,
    "LESS_THAN": lambda a: 1.0 if a[0] < a[1] else 0.0,
    "COMPARE": lambda a: 1.0 if abs(a[0] - a[1]) <= a[2] else 0.0,
}


def fold(op, a):
    """The value of Math `op` over constants, or None to leave it to a node."""
    rule = FOLDS.get(op)
    if rule is None:
        return None
    try:
        return rule(a)
    except (OverflowError, ValueError, ZeroDivisionError):
        return None


def build_material(material, doc, base_dir):
    """Replaces `material`'s node tree with the IR document `doc`; returns the warnings."""
    return Builder(material, doc, base_dir).build()
