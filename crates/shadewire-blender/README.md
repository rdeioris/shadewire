# shadewire-blender

The Blender front end of [Shadewire](https://github.com/rdeioris/shadewire): reads the `shadewire-blender-graph` JSON
document the Blender extension writes (File > Export > Material Graph (Shadewire JSON), or the
`shadewire.export_graph` operator) and lowers one material to Shadewire's IR through `shadewire-nodes`: a
Principled BSDF becomes `open_pbr_surface`, Blender's lobes a closure tree, its math, color, texture and procedural
nodes the matching MaterialX nodes.

The document holds a material's node tree as Blender has it: every node by `bl_idname`, its sockets by identifier
with their values, its enum properties by RNA identifier, the links, the node groups it uses and the images it names.
Groups are inlined, reroutes followed, muted nodes bypassed through their internal links and frames ignored before
anything is lowered.

```rust
use shadewire_blender::{IrOptions, to_ir};

let text = std::fs::read_to_string("material.json")?;
let ir = to_ir(&text, None, &IrOptions::default())?;
print!("{}", ir.dag.dump());
for warning in &ir.warnings {
    eprintln!("{warning}");
}
```

`to_ir` picks the document's only material when given `None` (`material_names` lists them), and lowers it for the
models `IrOptions` accepts. Every socket value a node reads becomes a named uniform (`{node}_{socket}`), so a host
can edit it without recompiling. `ir.warnings` lists what was dropped or approximated, one line per node and cause:
`ShaderNodeTexNoise "Noise Texture": Distortion dropped`.

## What maps

| | |
|---|---|
| Surface | Principled BSDF → `open_pbr_surface`, socket by socket. Under the `surface` model, Blender's lobes are a closure tree: Diffuse (`oren_nayar_diffuse_bsdf`), Glossy (`generalized_schlick_bsdf`), Glass and Refraction (`dielectric_bsdf`), Translucent, Sheen, Subsurface Scattering, Metallic (`generalized_schlick_bsdf` with F82 tint, or `conductor_bsdf`) and Emission (`uniform_edf`), under Mix Shader (`mix`) and Add Shader (`add`). Transparent BSDF is opacity: mixed with a Principled BSDF it scales `geometry_opacity`, mixed or added with lobes it passes its share of the light to what is behind: the lobes keep the weights Blender gives them, the surface's opacity is one minus the transparency, and the tree is divided by that opacity so that MaterialX's `surface` scales it back. Without the `surface` model, a lobe tree is `Error::NoTranslation`. |
| Patterns | Math, Vector Math, Clamp and Map Range in Blender's formulas (safe division, truncated modulo, round half up, Map Range's stepped and smooth modes). Mix in every blend mode, Separate/Combine Color and XYZ, Invert, Gamma, Brightness/Contrast, Hue/Saturation/Value, RGB and Value. Image Texture (`image`, with its extension as the address mode and its interpolation as the filter), Texture Coordinate, UV Map, Attribute, Color Attribute, Geometry, Normal Map (`normalmap`), Bump (written out from screen derivatives), Mapping and Tangent. Noise (`fractal3d`, or `noise2d` octaves in 2D), Voronoi (`worleynoise`), White Noise (`cellnoise`), Checker (`checkerboard`), Gradient and Light Path. |
| Not lowered | Every other node: each output read is the node's default output value and a warning. RGB Curves and Float Curve pass their input through. |

## Approximations

Every approximation that loses something is a warning on the node it touches; the implicit conversions are
Blender's own rules and silent.

- Blender's implicit conversions: a color read as a number is its luminance, a vector its average, a number a gray
  color or a splatted vector. A linked number read as an integer is floored, with a warning.
- Division is safe (anything over zero is zero) on the Math node only; Vector Math divides as MaterialX does.
- Generated coordinates are the object position; an unlinked Noise, Voronoi, White Noise or Gradient Vector is the
  object position too, an unlinked Checker Vector the first UV set. Geometry's True Normal is the shading normal.
- An Image Texture's projection is always Flat; an image with no file path is written by its name.
- UV Map, Normal Map, Tangent and Color Attribute read the first UV or color set, whatever map or layer they name.
- Glossy BSDF is a Schlick lobe tinted by its color; Glossy's and Metallic's Rotation and Glass's and Metallic's
  Thin Film are dropped.
- Separate and Combine Color read HSL as HSV; Map Range over vectors is linear.
- Noise, Voronoi and White Noise are MaterialX's noises, not Blender's: the same shapes, other values. Noise is
  fBM (other types are read as fBM, Distortion dropped, a linked Detail is 3 octaves, 1D and 4D read as 3D);
  Voronoi is the F1 or F2 Euclidean distance (other features read as F1, other metrics as Euclidean, Normalize
  dropped, Position the input point); the Color of Voronoi and White Noise is a gray cell noise.
- Checker is 2D: it checks over x and y only.
- Normal Map works in tangent space only; object and world space maps are read as tangent space.
- Displacement and Volume on the Material Output are ignored.
- The Principled BSDF's Anisotropic Rotation is dropped (OpenPBR rotates the tangent instead).
- A Principled BSDF mixed or added with other BSDFs, or with a second Principled BSDF, wins: the other side is
  dropped, with a warning (a Transparent BSDF mixed in scales its opacity instead).
- Attribute reads geometry attributes only: another attribute type, or no name, is zero; its Alpha output is 1. An
  unknown Gradient type is Linear. Each with a warning.
- Light Path is constants: Is Camera Ray 1, the rest 0.
- A Transparent BSDF is as transparent as its color's luminance. Where a closure tree lets all the light through
  (opacity 0), its lobes are not seen: an Add Shader of a lobe and a white Transparent BSDF shows only what is behind.

## Testing

`tests/fixtures.rs` lowers every document under `blender/fixtures/graphs` (dumps the Blender extension exported
from Blender itself) for the mapped models and for closure trees, validates the WGSL and compiles every back end;
its warnings and the MaterialX each material writes are `insta` snapshots in `tests/snapshots`. `tests/roundtrip.rs`
holds the front end to the hub's promise: `blender → ir → mtlx → ir` is the same material as `blender → ir` (same
uniforms, textures, closure tree and values at four probes), written with library nodes collapsed and inlined.
