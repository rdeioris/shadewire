# Shadewire MaterialX for Blender

A Blender extension (4.2 or later) that imports MaterialX materials as Cycles node trees. Shadewire compiles
each material: pattern graphs, named models (`standard_surface`, `open_pbr_surface`, `gltf_pbr`,
`UsdPreviewSurface`, `disney_principled`) opened into their lobes, and raw BSDF graphs. The add-on turns the
result into nodes: Math and Vector Math for the patterns, Image Texture for images (raw texels: Shadewire's
nodes convert colorspaces), and Diffuse, Glossy, Glass, Refraction, Metallic, Sheen, Subsurface, Translucent and
Emission shaders for the lobes, combined with Mix and Add Shader.

## Install

1. Build the compiler: `cargo build -p shadewire-cli --release` (it lands in `target/release/shadewire`).
2. Zip the `shadewire_mtlx` folder and install it from Blender's Preferences > Get Extensions > Install from
   Disk, or copy the folder into Blender's extensions directory.
3. In the add-on's preferences, point "shadewire executable" at the compiler, or copy it into
   `shadewire_mtlx/bin/` (or put it on PATH).

## Use

- **File > Import > MaterialX (Shadewire)** creates one Blender material per material of a `.mtlx` document.
  "Editable Values" keeps authored values as Value and RGB nodes; off, they are baked in and the lobes they
  turn off are left out.
- **Fill Materials from MaterialX** (Object menu, and the Material properties panel) rebuilds the materials
  already in the file whose names match the document's materials. Blender's USD importer makes an empty
  material for every binding into a `.mtlx` reference (`references = @./scene.mtlx@</MaterialX>`), so import
  the USD, then fill from the `.mtlx`.
- **Rebuild from MaterialX** (Material properties) compiles a material again from its document.

## Scripts

- `blender -b -P blender/render_usd.py -- scene.usd materials.mtlx out.png --samples 1024` renders a USD scene
  with Cycles after filling its materials.
- `blender -b -P blender/smoke_test.py -- [--shadewire PATH] [file.mtlx ...]` builds every material of the
  given documents (default: the vendored MaterialX examples) and reports failures.

## Approximations

- Layering: a `layer` base receives one minus the top's albedo (Fresnel at the shading normal for dielectric
  tops), as MaterialX's GLSL does.
- `generalized_schlick_bsdf` reflection uses the Metallic BSDF's F82 model: `color90` and `exponent` are not
  used.
- Noise, fractal, cell and Worley noise use Blender's noise textures: the same kind of pattern, other values.
- Weights that tint a lobe without a color input (conductor, Schlick) scale it by their luminance.
- Not translated yet: hex tiling, hair, conical and measured emitters, volumes.
