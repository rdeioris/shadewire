# Changelog

## Unreleased

- **Distilling closure trees to `open_pbr_surface`.** `IrOptions::distill`, with `open_pbr_surface` accepted, turns a
  material that would open into a closure tree (every MDL material, a pbrlib `surface`, a named model the host does
  not accept) into an `open_pbr_surface` whose fields are IR expressions over the tree's lobes and mix, layer and
  multiply weights (`shadewire_nodes::surface::distill`), after the MDL SDK distiller's `ue4` and
  `transmissive_pbr` targets: hosts that light a fixed model, such as Unreal base materials, take Omniverse's
  `OmniPBR`, `OmniGlass` and `OmniSurface`. Canonical shapes (a diffuse, a dielectric over a diffuse,
  open_pbr_surface's own tree) distill exactly; lobes without a field are dropped with an `approximations` entry
  naming them. CLI: `--distill`. C: `bool distill` appended to `shadewire_mtlx_options` and `shadewire_ir_options`
  (callers with the older `struct_size` read false).
- **MDL: typed arguments.** `shadewire_mdl::to_ir_with_args` takes `MdlArgument`s (floats, int, bool, string,
  texture with a colorspace) in place of a material's parameter defaults, converted to each parameter's declared type
  by MDL's rules; an unknown name or a mismatched type is an error naming the parameter. `Modules::to_ir_with_args`
  does the same on loaded modules. C: `shadewire_mdl_to_ir_args` and `shadewire_mdl_arg` (a `shadewire_value`,
  `is_texture`, `colorspace`), passed with `sizeof(shadewire_mdl_arg)` so the struct can grow.

## 0.3.0 (2026-10-17)

Since 0.2.0.

### Highlights

- **NVIDIA MDL, both ways.** `shadewire-mdl` reads MDL modules (1.0 to 1.11) with a compile-time evaluator: the
  chosen material runs symbolically, known values fold, varying ones become IR nodes, calls inline, run-time branches
  are predicated and loops unroll. The standard modules are NVIDIA's declarations with their meaning in Rust;
  `::base` and `::nvidia::core_definitions` are vendored and evaluated (the noise primitives are MaterialX's noises,
  with a warning). The `::df` tree becomes a closure tree of MaterialX's lobes, parameters become uniforms with their
  ranges and groups as UI hints, and what is dropped or approximated is in `MaterialIr.warnings`. The writer
  (`shadewire_mdl::compile`) prints any closure tree or `surface_unlit` material as a self-contained MDL 1.7 module
  that imports only the standard modules; every vendored MaterialX example passes the MDL SDK's `mdlc` and reads back
  to the same lobes.
- **`Modules`: load once, lower many times.** `shadewire_mdl::Modules` parses each module once with its imports and
  lowers its materials as often as asked, from any number of threads, with `Argument`s in place of parameter
  defaults. A module is named (`::a::b`) or given as a `.mdl` path.
- **CLI:** `.mdl` input wherever a `.mtlx` is read (`--format mdl`), `--mdl-path DIR` for the MDL search path, and
  `--lang mdl` (closure trees by default).
- **Python:** `shadewire.mdl` (`to_ir`, `material_names`, `compile`).
- **Graph builder:** build a MaterialX document in memory from Rust (`DocumentBuilder`) or C (`shadewire_builder_*`),
  with typed values and host source paths in messages (`shadewire_error_source_path` returns one).
- **C:** `shadewire_mdl_to_ir`, `shadewire_mdl_material_names` and the `SHADEWIRE_TARGET_MDL` target.

### Breaking changes

- The support libraries are pointer-free, so a language without out parameters can print them:
  `shadewire_floorfrac(x)` returns the fraction alone and `shadewire_bjmix` returns its three words. The generated
  code computes the same values; snapshots of the noise functions moved.
- MaterialX: an element named twice among its siblings in one document (nodes, nodegraphs, outputs, interface
  inputs, a node's inputs) is now an error; included documents merge first-wins, so a diamond include reads.
- `shadewire::Location` has a new public field, `source` (the host's source path, shown in place of file and line):
  breaking for struct literals and exhaustive patterns of `Location`.
- Colorspace names: USD and OCIO aliases (`lin_srgb`, `*_scene`, ACES names, …) resolve to MaterialX's cmlib
  colorspaces.

### MDL reader (`shadewire-mdl`)

- Imports: an unprefixed import looks beside the importing module first; a file off the search path resolves its
  relative and unprefixed imports against its own folder, and is keyed by its path.
- A search-path module that re-exports its own name (Kit's `::base` stub) re-exports the vendored one.
- A failed load is kept and leaves nothing behind; a load that panics leaves `Modules` an error for every later call.
- Hostile modules fail with `Error::Limit`: statements and expressions nest at most 2048 levels, and a material runs
  at most a million statements and calls.
- A top-level variant takes its base's parameters; a cycle of variants is an error.
- A declaration with constructor arguments (`bsdf b(...)`) copies a value of its type.
- A value a run-time select cannot hold, chosen by uniforms, is fixed at the uniforms' values.
- Weighted layers mix the layered top with their base, the weight limited to [0, 1].
- The writer never gives a parameter one of its own names, and every identifier starts with a letter.
- Opt-in test corpora: the MDL SDK's sources and example materials, and Kit's core library.

## 0.2.0 (2026-10-15)

Since 0.1.0.

### Highlights

- **`shadewire-nodes`, the node library as a crate.** The vendored libraries, nodedef resolution and the elaborator
  move out of `shadewire-mtlx`, which is now the MaterialX reader alone. A front end without a document
  (`Elaborator::standalone`) asks for nodes by name (`instantiate`) and finishes a material as a model or a closure
  tree (`finish_model`, `finish_closure`). The IR records the library nodes it was built from (`Dag::compounds`).
- **A MaterialX writer.** `shadewire_mtlx::from_ir` writes any IR back as a 1.39 document, library nodes as single nodes
  through the IR's compounds (`shadewire mtlx`, `shadewire.mtlx.from_ir`, `shadewire_ir_to_mtlx`); IR-only nodes
  (hextile, derivatives) are Shadewire nodedefs the reader also accepts.
- **A Blender front end.** `shadewire-blender` reads the material graph JSON the Blender extension writes and lowers
  it through `shadewire-nodes`: a Principled BSDF is `open_pbr_surface`, Blender's BSDFs a closure tree, its math,
  color, texture and procedural nodes the matching MaterialX nodes, with a warning for every approximation
  (`MaterialIr.warnings`).
- **Blender: material graphs as JSON and MaterialX export.** File > Export/Import > Material Graph (Shadewire JSON)
  saves a material's node tree exactly as Blender holds it and loads it back (`shadewire-blender-graph` version 1);
  the fixtures under `blender/fixtures/graphs` are such dumps. File > Export > MaterialX (Shadewire) writes materials
  through the Blender front end and the MaterialX writer; the USD export dialog's "Shadewire MaterialX" option
  replaces Blender's shader prims with a reference into a .mtlx written beside the .usd.
- **Bevy lights closure trees with MaterialX's lobes**, and builds every model by default (below).
- Every back end also writes `shadewire_eval_ambient` for a closure tree: the albedo split into diffuse and specular,
  for a host's own environment lighting.

### Breaking changes

No compatibility shims: old names are gone.

- **Rust:** `shadewire_mtlx::MtlxOptions` and `ElaborateOptions` are `shadewire_nodes::IrOptions`; `to_ir` takes
  the `Resolver` as its own argument. `shadewire_mtlx::{document, elaborate, element, index, library, parse,
  resolve_nodedef, surface, version}` are `shadewire_nodes::…`. `MaterialIr` lives in `shadewire-nodes`
  (re-exported by `shadewire-mtlx`) and carries `warnings`.
- **Rust:** `LOBE_MODELS` moved from `shadewire_gdshader` to `shadewire`: it picks what the front end accepts, on
  every back end.
- **Bevy:** `BuildOptions::mtlx` is `ir`; its include resolver is the new `BuildOptions.resolver`.
- **CLI:** `--lobes` opens every material into its closure tree on every back end (it was Godot only) and conflicts
  with `--model`; `ir` takes it too. `--transparency` and `--emission-scale` are an error without `--lang gdshader`.
- **IR export JSON:** a `compounds` list (format version unchanged: readers may ignore it).

### Core (`shadewire`)

- `export`: a `sample` whose lookup takes explicit gradients (hextiling) carries `grad: [dx, dy]`, the node indices
  of its uv derivatives; the field is optional and new within format version 1, so a reader that ignores it samples
  with implicit derivatives as before.
- `Op::Fractal2d` (`fractal2d` in the JSON): MaterialX's `fractal2d`, every width, from genglsl `mx_fractal2d_*`.
- Constant folding leaves an op alone when its value would be a NaN or an infinity (`normalize` of a zero vector):
  no JSON or shading language can hold that constant, and the GPU decides.

### MaterialX front end (`shadewire-mtlx`, `shadewire-nodes`)

- The compositing nodes upstream implements only in genglsl (`plus`, `minus`, `difference`, `screen`, `burn`,
  `dodge`; float, color3, color4) lower through Shadewire nodegraphs of existing ops.
- `viewdirection` has a nodedef (`ND_viewdirection_vector3`, world space by default): the specification describes
  it but upstream's libraries ship none.
- A 1.38 `normalmap` with `space="object"` upgrades to `normalize(in * 2 - 1)`, as upstream's upgrade writes it,
  instead of being refused.

### Bevy (`shadewire-bevy`)

- Closure trees (the `surface` model) lit by MaterialX's lobes: Bevy's point, spot and directional lights, shadows
  and contact shadows through `shadewire_eval_bsdf`, in Bevy's light units (the `lobe_calibration` example checks a
  white Lambert lobe against `StandardMaterial` under a directional light, the ambient light and an environment map).
  Indirect light through `shadewire_eval_ambient`'s diffuse and specular split, with nothing to invert: lightmaps,
  irradiance volumes, environment maps and the ambient light. `matlib_render --lobes` renders that way.
- The builder accepts `shadewire_bevy::MODELS` by default, `surface` included, so raw BSDF graphs,
  `UsdPreviewSurface` and `disney_principled` build (as closure trees) instead of failing; `BuildOptions::lobes()`
  opens every material into its lobes, and `gltf_mtlx --lobes` shows it.

### Python, CLI and C API

- `shadewire.blender.to_ir` and `.material_names`; `Ir.warnings`; `shadewire.mtlx.from_ir`.
- `shadewire compile|ir|list|mtlx` read Blender material graphs (`.json`, or `--format blender`); a front end's
  warnings print to stderr. `shadewire mtlx` writes the IR back as MaterialX.
- C API: `shadewire_blender_to_ir` (with `shadewire_ir_options`, the MaterialX options without the resolver),
  `shadewire_blender_material_names`, `shadewire_ir_warnings`, `shadewire_ir_to_mtlx`.

### Packaging

- Eleven crates on crates.io: `shadewire-nodes` and `shadewire-blender` are new. `shadewire-nodes` embeds
  MaterialX's libraries from its own folder (`crates/shadewire-nodes/materialx`).
- macOS Intel wheels build on `macos-15-intel` (GitHub retired `macos-13`).
- `blender/render_preview.py` renders the extensions.blender.org preview image.

### Testing

- The GPU parity test compiles one module per op family: a single module of every case aborted the NVIDIA Vulkan
  driver's compiler.
- Oracle materials for `fractal2d`, the compositing nodes and a 1.38 object-space `normalmap`.
- Blender: the smoke test round-trips material graphs and re-exports every example; the Blender front end has
  per-fixture round trips and MaterialX snapshots.

## 0.1.0 (2026-10-13)

Since `f5039f8` (readme: c example compiles).

### Highlights

- **Four native back ends, no naga.** WGSL, GLSL, HLSL and Metal (MSL) are each a crate of their own that prints
  one shared shader tree. The material's code, the support libraries and everything the languages have in common
  (operator precedence, declaration order, identifiers valid in all four) live in the core crate
  (`shadewire::shader`). naga no longer translates anything; it remains a test-only WGSL validator.
- **Closure trees and MaterialX's own lobes.** A host that accepts the `surface` model gets the material as a
  closure tree (lobes under layer, mix, add and multiply), and every back end writes the functions that evaluate it
  as MaterialX's reference GLSL does: `shadewire_eval_bsdf`, `shadewire_eval_albedo`, `shadewire_eval_emission`.
- **A Bevy integration**, a **Blender add-on** that imports MaterialX as Cycles node trees, and a **Python
  module** with wheels for every Blender platform.

### Breaking changes

No compatibility shims: old names are gone.

- **Rust:** `Compiled` and `CompileOptions` live in `shadewire::shader` (re-exported by every back end);
  `Compiled.wgsl` is `Compiled.code`. `shadewire-wgsl` loses `translate`, `link`, `Target` and its versions,
  `glsl_library`, `reference_host`, the `emit` module and the `translate` feature. `shadewire::Error::Naga` is gone.
- **CLI:** `--lang` is `wgsl|glsl|glsl-es|hlsl|msl` and picks the back end; `--glsl-version N` sets the GLSL
  version. No SPIR-V, and no reference host linked in.
- **C API:** `shadewire_compile(ir, target, options, ...)` replaces `shadewire_wgsl_compile`;
  `shadewire_compile_mtlx` is gone (`shadewire_mtlx_to_ir`, then `shadewire_compile`); `shadewire_compiled_code`
  replaces `shadewire_compiled_wgsl`. `shadewire_translate`, the blob type, `SHADEWIRE_TARGET_SPIRV`, `_WEBGL2` and HLSL
  shader model versions are gone; target kinds are renumbered (WGSL 0, GLSL 1, GLSL ES 2, HLSL 3, MSL 4).
  `SHADEWIRE_ERROR_SHADER` is gone and the error kinds after it are renumbered (IO 6, PANIC 7).
- **Generated code:** WGSL comes from the shared tree: typed `let`s, minimal parentheses, library functions in
  dependency order. Field names avoid the reserved words of all four languages, so a field called `sample` is
  `sample_` everywhere (the reflection keeps the authored names).
- **GLSL bindings** are reflection's numbers (the uniform block at `uniform_binding`, each texture at unit
  `binding`), no longer `32 * group + binding`.

### Core (`shadewire`)

- `shader`: the typed shader tree (`ast`), a parser and type inference for the WGSL subset the support libraries
  are written in, the libraries themselves (`shader/lib`: MaterialX genglsl helpers, noise, hextiling and the pbrlib
  lobes), the material emitter (`material`), the shared compile steps (`lower`, `compile_with`), the printing every
  back end shares (`print`, with a `Syntax` trait and C-family defaults) and test materials (`fixtures`).
- Closure trees for the `surface` model (`ClosureGraph`): lobes and their combinations, with every value a surface
  field. The optimizer prunes closures whose weights fold to zero.
- `export`: the IR as a flat JSON document (`shadewire ir --json`, Python `Ir.to_dict`).
- Geomprop defaults, covered by `structure_hash`.
- `CompileOptions`: `reserve_uniform_binding` keeps the first binding for the uniform block even without uniforms;
  `implicit_derivatives: false` makes the code run in a compute stage (level-0 samples, zero derivatives).

### Back ends

- **WGSL** (`shadewire-wgsl`): the shared tree printed as WGSL; `validate` (feature) checks it with naga.
- **GLSL** (`shadewire-glsl`, new): a GLSL library, desktop from 330 and ES from 300. Combined `sampler2D`s, an
  std140 uniform block, host function prototypes, explicit bindings from GLSL 420 and ES 310.
- **HLSL** (`shadewire-hlsl`, new): an HLSL library for shader model 5.1 and up. `ConstantBuffer` and textures at
  `register(b|t|s{binding}, space{group})`, the uniform block padded to the reflected offsets.
- **MSL** (`shadewire-msl`, new): a Metal library. The functions that read the material's resources take them as
  trailing parameters (Metal has no global resources); the uniform block is padded, with `packed_float3`.
- **Lobes:** WGSL ports of MaterialX's pbrlib GLSL: Oren-Nayar (plain and energy-compensated), Burley,
  translucent, dielectric, conductor and generalized Schlick GGX with energy compensation, exact and Hoffman-Schlick
  Fresnel, thin-film iridescence, Conty-Kulla and Zeltner sheen; uniform, conical, measured and generalized Schlick
  EDFs. The albedo is MaterialX's response to a uniform white environment. Subsurface scatters as Lambertian diffuse;
  hair and volume lobes are not evaluated.

### MaterialX front end (`shadewire-mtlx`)

- pbrlib surface nodes arrive as closure trees for hosts that accept the `surface` model; named models a host does
  not list open through their implementation graphs.
- Stdlib nodegraphs for helpers upstream implements per language (`artistic_ior`, `roughness_anisotropy`,
  `roughness_dual`, `rotate3d`); `subsurface_bsdf`'s radius upgrades to color3.

### Bevy (`shadewire-bevy`, new)

- MaterialX as a `ShadewireMaterial`: a `StandardMaterial` extended with a fragment shader generated for the graph
  (`ShadewirePlugin`, `FromMtlx`). Identical graphs share one shader and pipeline.
- Back faces, coat and transmission turned on in the base when the graph uses them, unlit premultiplied color, a
  cache key from the structure hash, and the `gltf_mtlx` example (a material on a glTF scene, live recompiling).
- Limits: forward rendering, up to 8 textures and 1 KiB of uniforms per material.

### Blender add-on (`blender/shadewire`, new)

- Blender 4.3 or later (conductors use the Metallic BSDF, new in 4.3); the extension's id is `shadewire`.
- File > Import > MaterialX builds Blender material node trees (rendered by Cycles and EEVEE) through Shadewire.
  Every vendored example builds, in both shading modes.
- USD import (Blender 4.4 or later): a `USDHook` builds the MaterialX materials of a USD scene as Blender imports
  it, from the referenced `.mtlx` (also inside a `.usdz`) or from the MaterialX network authored in the USD,
  textures unpacked by the importer. "Fill Materials from MaterialX" rebuilds materials by name for scenes
  imported without it.
- Shading **Principled BSDF** (the default): a `standard_surface`, `open_pbr_surface` or `gltf_pbr` material drives
  one Principled BSDF from its parameters; other models and raw BSDF graphs become their lobes. **MaterialX
  Lobes**: every material as its lobes, one BSDF node each, layered and mixed as MaterialX defines them.
- Uses the `shadewire` Python module (or the CLI); ships one extension zip per platform with its wheels
  (`blender/build_extension.py`).
- Licensed GPL-3.0-or-later, as extensions.blender.org requires of add-ons; everything else stays MIT.

### Python (`shadewire`, new)

- PyO3 against Python's stable ABI from 3.8: one wheel per platform for every CPython from 3.8 (Windows x64/ARM64,
  macOS x64/arm64, manylinux2014 x64/aarch64), built by `.github/workflows/wheels.yml`.
- `shadewire.mtlx` (`to_ir`, `material_names`), `shadewire.Ir` (`optimize`, `dump`, `to_dict`), and
  `shadewire.wgsl`, `.glsl`, `.hlsl`, `.msl`, each a `compile` to `shadewire.Compiled` (`code`, `language`,
  `reflection`, `model_struct`).
- Examples: `wgsl_lobes.py` (a whole WGSL shader around a material), `wgpu_cube.py` (wgpu-py), `gl_cube.py`
  (pygame and OpenGL, a host that takes the `surface` model and lights MaterialX's lobes) and `gl_pbr_cube.py` (the
  same window with the renderer's own metal-roughness BRDF: it takes the named models `standard_surface`,
  `open_pbr_surface` and `gltf_pbr` and maps each model's parameters onto its inputs).

### CLI and C API

- The C API follows the Python module name for name: `shadewire_mtlx_material_names` (a `shadewire_strings` list),
  `shadewire_ir_optimize`, `shadewire_ir_json` (`Ir.to_dict`, format `SHADEWIRE_IR_FORMAT_VERSION`),
  `shadewire_ir_source`, `_source_version`, `_structure_hash`, `shadewire_compiled_language`, and
  `implicit_derivatives` in the compile options. `shadewire_compiled_struct_name` is now
  `shadewire_compiled_model_struct`.
- GLSL versions follow one rule everywhere (`shadewire_glsl::Version::check`): desktop 330 and 400 to 460, ES 300
  to 320. Another version is `Error::UnsupportedTarget` in Rust, `ShadewireError` in Python, an error in the CLI and
  SHADEWIRE_ERROR_INVALID_ARGUMENT in C.
- Python's `Compiled.ir`: the IR the code was printed from.
- `shadewire list` prints a document's materials; `shadewire ir --json` writes the IR document.
- `--reserve-uniform-binding` in the CLI, `reserve_uniform_binding` in the C API's compile options.
- C API includes through the host: `shadewire_mtlx_options` gains `resolve` (a `shadewire_resolve_fn` answering with
  `shadewire_resolution_set` / `_fail`) and `resolve_user_data`; `shadewire_mtlx_list_dependencies` lists a
  document's includes and texture files (`shadewire_dependencies`). Breaking: the `path` parameters are `location`,
  and `shadewire_mtlx_material_names` takes the MaterialX options (for the resolver) after it.
- Python includes through the host, as in C: `shadewire.mtlx.to_ir` and `material_names` take `resolver`, a
  callable `(base, href) -> (location, text)` that raises to fail (called with the GIL; the exception's message is
  the error), and `shadewire.mtlx.list_dependencies(text, location=None)` returns `{"includes", "files"}`. Tests in
  `crates/shadewire-py/tests` (pytest, against an installed module).

### Packaging

- Ready for crates.io (nine crates, `cargo publish --workspace`), PyPI (six abi3 wheels and the sdist) and
  extensions.blender.org (one zip holding the wheel of every platform Blender ships for): see `RELEASING.md`.
- `shadewire-mtlx` embeds MaterialX's libraries from its own folder (`crates/shadewire-mtlx/materialx`).
- `rust-version` is 1.88; `shadewire-bevy` needs 1.97.

### Testing

- Every back end compiles the shared test materials with the real compiler when installed: naga (WGSL),
  glslangValidator (GLSL 330, 420, 450, ES 300, 310), dxc (HLSL as a library and as a pixel shader, HLSL 2018 and
  2021) and Apple's Metal compiler (MSL). `SHADEWIRE_REQUIRE_TOOLS=1` makes a missing one a failure.
- The MaterialX acceptance test compiles all 54 vendored examples with every back end and validates the code.
- The GPU parity test (interpreter against the generated WGSL) passes on Windows (Vulkan) and macOS (Metal).

### Known limitations

- Light arriving from behind the surface is not transmitted through dielectrics (as in MaterialX's GLSL); no
  environment lighting or direction sampling yet.
- No translation between named models: a host gets a material only in its authored model (a `standard_surface`
  material is refused by a host that accepts only `gltf_pbr`).
- No SPIR-V output.
