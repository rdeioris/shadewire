# Shadewire MaterialX for Blender

A Blender extension (4.3 or later: conductors use the Metallic BSDF) that imports MaterialX materials as Blender
material node trees, which Cycles and EEVEE both render. Shadewire compiles each material; the add-on turns the
result into nodes: Math and Vector Math for the patterns, Image Texture for images (raw texels: Shadewire's nodes
convert colorspaces), Geometry, UV and Attribute nodes for what the material reads of the mesh, and then the
shading, in one of two modes:

- **Principled BSDF** (the default): a material in a named model (`standard_surface`, `open_pbr_surface`,
  `gltf_pbr`) drives one Principled BSDF, its inputs linked to the model's parameters. Small, editable, and what
  EEVEE handles best. Other models (`UsdPreviewSurface`, `disney_principled`) and raw BSDF graphs, which have no
  Principled counterpart, become their lobes as below.
- **MaterialX Lobes**: every material opens into its lobes, one Diffuse, Glossy, Glass, Refraction, Metallic,
  Sheen, Subsurface, Translucent or Emission node each, combined with Mix and Add Shader as MaterialX layers and
  mixes them. Closest to MaterialX, in Cycles; EEVEE evaluates only a few BSDF nodes per material.

## License

The add-on (`shadewire`, the Python code that drives Blender) is licensed GPL-3.0-or-later (see
`shadewire/LICENSE`), as extensions.blender.org requires of add-ons. The `shadewire` module it bundles, like the
rest of Shadewire, is MIT, with parts under Apache-2.0 and BSD-3-Clause (see the repository's NOTICE); all three are
compatible with the GPL.

## Install

The extension bundles the compiler as the `shadewire` Python module (`crates/shadewire-py`): one wheel per
platform, built for Python's stable ABI from 3.8, so it loads in every Blender from 4.3 on. One extension zip
holds the wheels of every platform Blender ships for (Windows x64 and ARM64, macOS Intel and Apple Silicon, Linux
x64; there is no Linux ARM64 Blender), and Blender installs its own; install it from Preferences > Get Extensions >
Install from Disk.

Build the wheels and the zips with `python blender/build_extension.py --blender <blender executable>` (it needs
`pip install maturin ziglang` and the Rust targets it lists). CI builds and tests the wheels on native runners
(`.github/workflows/wheels.yml`).

From a source checkout, without the module, the add-on falls back to the `shadewire` command-line compiler
(`cargo build -p shadewire-cli --release`): set its path in the add-on's preferences, put it in
`shadewire/bin/`, or on PATH. A path in the preferences always wins over the module.

## Use

- **File > Import > MaterialX (Shadewire)** creates one Blender material per material of a `.mtlx` document.
  "Shading" picks Principled BSDF or MaterialX Lobes. "Editable Values" keeps authored values as Value and RGB
  nodes; off, they are baked in and the lobes they turn off are left out.
- **File > Import > Universal Scene Description** (Blender 4.4 or later) builds the scene's MaterialX materials
  as it imports them: the add-on's USD hook claims every material with a MaterialX (`mtlx`) surface, which
  Blender's importer would leave empty. A material referenced whole from a `.mtlx` file
  (`references = @./scene.mtlx@</MaterialX>`) compiles from that document, read through USD's resolver, so a
  `.mtlx` packed in a `.usdz` works too; a MaterialX network authored in the USD (or a `.mtlx` material that USD
  layers override) is written back as a MaterialX document first. Textures come through the importer, which
  unpacks them from a `.usdz`. The add-on's preferences turn it off and pick its Shading and Editable Values.
- **Fill Materials from MaterialX** (Object menu, and the Material properties panel) rebuilds the materials
  already in the file whose names match the document's materials: for scenes imported without the hook (Blender
  4.3, or the hook turned off).
- **Rebuild from MaterialX** (Material properties) compiles a material again from its document, in the mode it
  was built with.
- **File > Export > Material Graph (Shadewire JSON)** (and Save Graph in the Material properties panel) writes
  materials as a JSON document: the selected objects' materials, the active one, or all of them, with their node
  groups and image references. "Copy Textures" writes the images into a `textures` folder beside the file, packed
  and generated ones as PNG, and points the document at them.
- **File > Import > Material Graph (Shadewire JSON)** (and Load Graph) creates the document's materials, node
  groups and images; "Replace Existing" rebuilds a material of the same name in place instead. Whatever this
  Blender cannot set (a node type or property it lacks) is skipped and reported as a warning.

The document holds the node tree as Blender has it, for Shadewire's front end and for moving materials between
files.

## Scripts

- `python blender/build_extension.py --blender <blender>` builds the six wheels (the PyPI set, in
  `<out>/wheels`) and the extension zip.

- `blender -b -P blender/render_usd.py -- scene.usd out.png --samples 1024 [--engine EEVEE] [--mode LOBES]` renders
  a USD scene, its materials built by the USD hook (with `shadewire` importable, or `--shadewire <executable>`).
- `blender -b -P blender/smoke_test.py -- [--shadewire PATH] [--mode LOBES] [--flatten] [file.mtlx|scene.usd ...]`
  builds every material of the given documents (default: the vendored MaterialX examples) and reports failures.
  USD scenes are imported through the USD hook: every MaterialX material must arrive built and every texture
  load; `--flatten` also imports each scene flattened, its MaterialX inline.
- `blender -b --python-exit-code 1 -P blender/test_graph.py` runs the graph tests: dump, load, round trips and the
  Save and Load operators. Keep the flag: without it Blender exits 0 when the script errors.
- `blender -b --python-exit-code 1 -P blender/make_fixtures.py` rewrites the graph fixtures in
  `blender/fixtures/graphs` from the materials it builds; `test_graph.py` fails when they are stale.

Both scripts load the add-on from `blender/shadewire` under the module name `shadewire_addon` (its folder is not put
on `sys.path`, where it would hide the `shadewire` compiler module). With `--installed` they use the extension
Blender has installed and enabled instead: install a zip into a throwaway profile and test what users get, bundled
wheel included:

```bash
export BLENDER_USER_RESOURCES=/tmp/shadewire-profile
blender -b --command extension install-file -r user_default -e shadewire-0.1.0.zip
blender -b -P blender/smoke_test.py -- --installed
```

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
