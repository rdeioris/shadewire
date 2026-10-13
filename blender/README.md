# Shadewire MaterialX for Blender

A Blender extension (4.2 or later) that imports MaterialX materials as Blender material node trees, which Cycles
and EEVEE both render. Shadewire compiles each material; the add-on turns the result into nodes: Math and Vector
Math for the patterns, Image Texture for images (raw texels: Shadewire's nodes convert colorspaces), Geometry,
UV and Attribute nodes for what the material reads of the mesh, and then the shading, in one of two modes:

- **Principled BSDF** (the default): a material in a named model (`standard_surface`, `open_pbr_surface`,
  `gltf_pbr`) drives one Principled BSDF, its inputs linked to the model's parameters. Small, editable, and what
  EEVEE handles best. Other models (`UsdPreviewSurface`, `disney_principled`) and raw BSDF graphs, which have no
  Principled counterpart, become their lobes as below.
- **MaterialX Lobes**: every material opens into its lobes, one Diffuse, Glossy, Glass, Refraction, Metallic,
  Sheen, Subsurface, Translucent or Emission node each, combined with Mix and Add Shader as MaterialX layers and
  mixes them. Closest to MaterialX, in Cycles; EEVEE evaluates only a few BSDF nodes per material.

## Install

The extension bundles the compiler as the `shadewire` Python module (`crates/shadewire-py`): one wheel per
platform, built for Python's stable ABI from 3.8, so it loads in every Blender from 4.2 on. Each platform has
its own extension zip (Windows x64 and ARM64, macOS Intel and Apple Silicon, Linux x64 and ARM64); install it
from Preferences > Get Extensions > Install from Disk.

Build the wheels and the zips with `python blender/build_extension.py --blender <blender executable>` (it needs
`pip install maturin ziglang` and the Rust targets it lists). CI builds and tests the wheels on native runners
(`.github/workflows/wheels.yml`).

From a source checkout, without the module, the add-on falls back to the `shadewire` command-line compiler
(`cargo build -p shadewire-cli --release`): set its path in the add-on's preferences, put it in
`shadewire_mtlx/bin/`, or on PATH. A path in the preferences always wins over the module.

## Use

- **File > Import > MaterialX (Shadewire)** creates one Blender material per material of a `.mtlx` document.
  "Shading" picks Principled BSDF or MaterialX Lobes. "Editable Values" keeps authored values as Value and RGB
  nodes; off, they are baked in and the lobes they turn off are left out.
- **Fill Materials from MaterialX** (Object menu, and the Material properties panel) rebuilds the materials
  already in the file whose names match the document's materials. Blender's USD importer makes an empty
  material for every binding into a `.mtlx` reference (`references = @./scene.mtlx@</MaterialX>`), so import
  the USD, then fill from the `.mtlx`.
- **Rebuild from MaterialX** (Material properties) compiles a material again from its document, in the mode it
  was built with.

## Scripts

- `python blender/build_extension.py --blender <blender>` builds the six wheels and the six extension zips.

- `blender -b -P blender/render_usd.py -- scene.usd materials.mtlx out.png --samples 1024 [--mode LOBES]` renders
  a USD scene with Cycles after filling its materials (with `shadewire` importable, or `--shadewire <executable>`).
- `blender -b -P blender/smoke_test.py -- [--shadewire PATH] [--mode LOBES] [file.mtlx ...]` builds every material
  of the given documents (default: the vendored MaterialX examples) and reports failures.

## Approximations

Principled BSDF mode:

- Blender's Principled BSDF tints subsurface and transmission with its base color, so the base color moves toward
  `subsurface_color` and `transmission_color` as far as their weights.
- Specular: `specular` (`specular_weight`) times 0.5 is the Specular IOR Level (Blender's 0.5 keeps the IOR's own
  reflectance).
- glTF's `occlusion` has no Principled input and is left out; `alpha_mode` mask thresholds alpha at `alpha_cutoff`.
- `thin_walled`, coat darkening and the volume parameters (transmission depth, scatter, attenuation) are left out.

MaterialX Lobes mode, and lobes in either mode:

- Layering: a `layer` base receives one minus the top's albedo (Fresnel at the shading normal for dielectric
  tops), as MaterialX's GLSL does.
- `generalized_schlick_bsdf` reflection uses the Metallic BSDF's F82 model: `color90` and `exponent` are not
  used.
- Noise, fractal, cell and Worley noise use Blender's noise textures: the same kind of pattern, other values.
- Weights that tint a lobe without a color input (conductor, Schlick) scale it by their luminance.
- Not translated yet: hex tiling, hair, conical and measured emitters, volumes.
