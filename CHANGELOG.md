# Changelog

## Unreleased

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
  `LOBE_MODELS` moved from `shadewire_gdshader` to `shadewire`: it picks what the front end accepts, on every back end.
- **CLI:** `--lang` is `wgsl|glsl|glsl-es|hlsl|msl` and picks the back end; `--glsl-version N` sets the GLSL
  version. No SPIR-V, and no reference host linked in. `--lobes` opens every material into its closure tree on every back end
  (it was Godot only) and conflicts with `--model`; `ir` takes it too. `--transparency` and `--emission-scale` are an error
  without `--lang gdshader`.
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
