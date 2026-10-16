<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo/shadewire-logo-dark.svg">
    <img alt="Shadewire" src="assets/logo/shadewire-logo.svg" width="420">
  </picture>
</p>

# Shadewire

Shadewire is a compiler for material graphs. A **front end** reads a material description
(MaterialX 1.38 and 1.39 today) into Shadewire's IR, a typed DAG that computes the material's surface; a
**back end** prints the IR as shader code in the language a renderer speaks: WGSL, GLSL, GLSL ES, HLSL, Metal,
or a complete Godot 4 shader. With the code comes reflection that says what the host must bind: the uniform
block and its offsets, the textures and their colorspaces, and whether each surface parameter is a constant,
a uniform, computed from uniforms, or varies per fragment.

A host takes a material in one of two ways:

- **As a shading model.** The host names the models it accepts (`open_pbr_surface`, `standard_surface`,
  `gltf_pbr`, `surface_unlit`; also `UsdPreviewSurface` and `disney_principled`) and the shader fills that
  model's struct in `shadewire_eval_surface()`, for the host to light with its own BRDFs. A material of another
  model is an error, not an approximation.
- **As a closure tree.** A host that accepts `surface` gets every material opened into its lobes (diffuse, dielectric,
  conductor, sheen, subsurface, emitters, under layer, mix, add and multiply) and the functions that evaluate them as
  MaterialX's reference implementation does: `shadewire_eval_bsdf`, `shadewire_eval_albedo`, `shadewire_eval_emission`
  (and `shadewire_eval_ambient` for a host lighting the environment its own way). The material then looks the same in
  every renderer that calls them.

The code calls host functions for geometry and time (`shadewire_host_normal_world()`,
`shadewire_host_texcoord_0()`, ...) that the renderer defines next to its lights and entry point; nothing in
the generated code depends on a particular engine. On top of the Rust crates sit the `shadewire` command, a C
API, a Python module (`pip install shadewire`), a Bevy plugin, a Godot 4 extension that loads `.mtlx` files as
`ShaderMaterial`s, and a Blender extension that imports MaterialX (and the MaterialX materials of USD scenes)
as Blender node trees.

Licensed under MIT; vendored MaterialX files are Apache-2.0 and the noise and color code derives from Open
Shading Language, BSD-3-Clause (see NOTICE). The Blender add-on (`blender/shadewire`) is GPL-3.0-or-later, as
extensions.blender.org requires. Pre-release: APIs change between versions without compatibility shims.

## Crates

Front ends and back ends never depend on each other. They share the core and, for front ends, the node library; the
caller (your code, the CLI, the C API) composes them.

| Crate | Role | Depends on |
|---|---|---|
| `shadewire` | core: the IR, optimizer, CPU interpreter, reflection, errors; the shader tree every back end prints (`shader`: the material's code, the support libraries, the shared printing) | — |
| `shadewire-nodes` | the node library: MaterialX's standard nodes and Shadewire's own, resolved and instantiated into the IR; what every front end lowers through (`Elaborator`, `instantiate`, `finish_model`, `finish_closure`) | `shadewire` |
| `shadewire-mtlx` | front end: MaterialX 1.38/1.39 documents → IR (`to_ir`) | `shadewire`, `shadewire-nodes` |
| `shadewire-blender` | front end: the Blender extension's material graph JSON → IR (`to_ir`) | `shadewire`, `shadewire-nodes`, `serde_json` |
| `shadewire-wgsl` | back end: IR → WGSL + reflection (`compile`) | `shadewire` |
| `shadewire-glsl` | back end: IR → a GLSL library (desktop from 330, ES from 300) + reflection | `shadewire` |
| `shadewire-gdshader` | back end: IR → a complete Godot 4 spatial shader (`.gdshader`) + reflection: named models mapped onto Godot's outputs, or closure trees lit by MaterialX's reference lobes in `light()` | `shadewire` |
| `shadewire-hlsl` | back end: IR → an HLSL library (shader model 5.1 and up) + reflection | `shadewire` |
| `shadewire-msl` | back end: IR → a Metal Shading Language library + reflection | `shadewire` |
| `shadewire-cli` | the `shadewire` command: MaterialX documents or Blender graphs (`.json`) in, any back end out | all of the above |
| `shadewire-capi` | C API (`shadewire.h`): MaterialX or Blender graphs → code in any back end's language + reflection | `shadewire`, `shadewire-mtlx`, `shadewire-blender`, `shadewire-nodes`, the back ends |
| `shadewire-bevy` | Bevy 0.20: MaterialX → a `StandardMaterial` extension with a generated fragment shader (`ShadewirePlugin`, `FromMtlx`) | `shadewire`, `shadewire-mtlx`, `shadewire-nodes`, `shadewire-wgsl`, `bevy` |
| `shadewire-godot` | Godot 4.4+ extension (gdext; its own workspace, Rust 1.94): `.mtlx` → `ShaderMaterial` at runtime (`Shadewire`, `load()`) and on import | `shadewire`, `shadewire-mtlx`, `shadewire-nodes`, `shadewire-gdshader`, `godot` |
| `shadewire-py` | the `shadewire` Python module (PyO3, stable ABI from CPython 3.8): `shadewire.mtlx`, `.blender`, `.wgsl`, `.glsl`, `.hlsl`, `.msl` | `shadewire`, `shadewire-mtlx`, `shadewire-blender`, `shadewire-nodes`, the back ends |

Each back end is a printer of the shared shader tree: everything they have in common (the material's code, the
support libraries, operator precedence, declaration order, identifiers valid in every language) lives in the core.
The Blender extension (`blender/shadewire`) is Python over the `shadewire` module. Planned: a glTF front end, more
back ends.

## What to depend on

| You want | Crates | Features |
|---|---|---|
| Inspect or print a material's IR | `shadewire`, `shadewire-mtlx` | — |
| Evaluate a material on the CPU (tests, baking, thumbnails) | `shadewire`, `shadewire-mtlx` | — |
| MaterialX → WGSL + reflection (wgpu, Bevy, WebGPU) | `shadewire-mtlx`, `shadewire-wgsl` | — |
| MaterialX → a Godot 4 shader (for a `ShaderMaterial`) | `shadewire-mtlx`, `shadewire-gdshader` | — |
| MaterialX materials in a Godot project or game | the `shadewire-godot` addon ([its README](crates/shadewire-godot/README.md)) | — |
| MaterialX → GLSL, GLSL ES, HLSL or MSL | `shadewire-mtlx`, `shadewire-glsl` / `-hlsl` / `-msl` | — |
| Blender material graphs → the IR | `shadewire-blender` and a back end | — |
| Your own IR (any source) → any of them | `shadewire` and the back end | — |
| Your own graph format → the IR | `shadewire`, `shadewire-nodes` | — |
| A command-line tool | `shadewire-cli` (the `shadewire` binary) | — |
| A C/C++ host | `shadewire-capi` (shared or static library + `shadewire.h`) | — |
| A Python host or tool | `pip install shadewire` ([its README](crates/shadewire-py/README.md)) | — |
| A Bevy app | `shadewire-bevy` | — |
| MaterialX materials in Blender | the Blender extension ([its README](blender/README.md)) | — |

## Usage

Library, two steps: front end, then back end.

```rust
use shadewire::optimize::optimize;
use shadewire_mtlx::{IrOptions, Resolver, to_ir};
use shadewire_wgsl::{CompileOptions, compile};

let text = std::fs::read_to_string("material.mtlx")?;
let ir = to_ir(&text, Some("material.mtlx"), None, &IrOptions::default(), &Resolver::default())?;
print!("{}", optimize(&ir.dag).dump()); // the IR, one node per line

let out = compile(&ir.dag, &ir.source, &CompileOptions::default())?;
println!("{}", out.code);             // fn shadewire_eval_surface() -> <ModelStruct>
println!("{}", out.reflection.model);
```

Every back end writes the same library in its language: the model's struct, the uniform block and textures,
`shadewire_eval_surface()`, and for a material with lobes the functions evaluating them as MaterialX defines
(`shadewire_eval_bsdf`, `shadewire_eval_albedo`, `shadewire_eval_emission`, and `shadewire_eval_ambient`, the albedo
split into diffuse and specular for a host with its own environment lighting). The code calls host functions
(`shadewire_host_texcoord_0()`, `shadewire_host_normal_world()`, ...), declared in GLSL, HLSL and MSL, which the host
defines in its own code next to its lights and entry point. Examples:
`cargo run -p shadewire-mtlx --example mtlx_to_wgsl -- material.mtlx` and `--example mtlx_to_hlsl`.

Command line:

```
shadewire compile material.mtlx --reflection material.json -o material.wgsl
shadewire compile material.mtlx --lang hlsl -o material.hlsl
shadewire compile material.mtlx --lang gdshader -o material.gdshader
shadewire ir material.mtlx               # print the IR (optimized; --no-optimize for the raw graph; --json)
shadewire list scene.mtlx                # the document's material names
shadewire mtlx material.mtlx [--material NAME] [--model M]... [--lobes] [--inline] [--optimize] [--constants] [-o FILE]
shadewire compile material.json --lang wgsl   # a Blender material graph
```

- Every command reads a Blender material graph (the JSON the Blender extension's File > Export > Material Graph writes) wherever it reads a `.mtlx`: a `.json` file is one, `--format mtlx|blender` overrides the extension. What the Blender front end dropped or approximated prints to stderr, one line per node.

- `shadewire mtlx` writes the material back as MaterialX: includes resolved, 1.38 upgraded, library nodes kept as nodes. `--inline` expands library nodes; `--optimize` folds constants first and writes leaf ops (the optimizer drops the compounds); `--constants` bakes authored values in as constants.
- `--material NAME` picks the material when the document has several.
- `--model MODEL` (repeatable) names the surface models the host accepts: an accepted model comes out as its struct, filled by `shadewire_eval_surface()`, for the host to light. By default `open_pbr_surface`, `standard_surface`, `gltf_pbr` and `surface_unlit`. A material of another model is an error, not a translation, unless `surface` is accepted: then it opens into its closure tree.
- `--lobes` accepts only `surface` and `surface_unlit`, so every material opens into its closure tree and the shader brings MaterialX's reference lobes to light it (`shadewire_eval_bsdf` and the rest), on every back end. It cannot be combined with `--model`; `ir` takes it too.
- `--lang gdshader` writes a complete Godot 4 `shader_type spatial;` file. Godot binds by name: uniforms keep their authored names, made valid identifiers clear of reserved and generated ones (`GdshaderOutput::uniform_names`); textures are `shadewire_t<index>`, bound raw since the shader converts colorspaces. By default named models (`open_pbr_surface`, `standard_surface`, `gltf_pbr`, `UsdPreviewSurface`, `disney_principled`, `surface_unlit`) are mapped onto Godot's outputs and lit by Godot's BRDF, and a pbrlib `surface` is lit by MaterialX's reference lobes in a generated `light()`; with `--lobes` every material is lit that way (ambient light and reflections then see the closure's albedo as diffuse). What Godot cannot represent is listed in the reflection's `approximations`. Scalar uniforms get `hint_range` from the soft limits, else the hard ones (each end separately); uniforms with a `uifolder` go in `group_uniforms` named after it (`/` becomes `.`, one subgroup level), the rest and the textures in `material`. `--transparency auto|opaque|scissor|hash|blend` (default `auto`: opaque for a constant opacity of 1, scissor for a constant glTF MASK or USD threshold, blend otherwise) and `--emission-scale` (default 1) apply to it only: with another `--lang` they are an error.
- `--lang wgsl|glsl|glsl-es|hlsl|msl|gdshader` picks the back end; `--glsl-version N` the GLSL version (default 420, 300 for `glsl-es`).
- `--reflection FILE` writes the reflection JSON; it is written only when the shader was produced.
- `--no-optimize` skips constant folding, simplification and sharing.
- `--constants` bakes authored values in instead of exposing them as uniforms.
- `--reserve-uniform-binding` keeps binding 0 for the uniform block even when the material has no uniforms, so textures start at binding 1 (for hosts that bind their own buffer there).

## C API

`cargo build -p shadewire-capi --release` writes the library to `target/release`: `shadewire_capi.dll` with its
import library `shadewire_capi.dll.lib` and the static `shadewire_capi.lib` on Windows; `libshadewire_capi.so` or
`libshadewire_capi.dylib` and the static `libshadewire_capi.a` elsewhere (a static link also needs the system
libraries listed by `cargo rustc -p shadewire-capi --release --crate-type staticlib -- --print native-static-libs`).
The header is `crates/shadewire-capi/include/shadewire.h`. Every handle and error has its own `_free`.

```c
#include <stdio.h>

#include "shadewire.h"

/* Compiles `len` bytes of MaterialX `text` and prints it as an HLSL library; returns 0 on success. */
int print_hlsl(const char *text, size_t len) {
    shadewire_ir *ir = NULL;
    shadewire_compiled *compiled = NULL;
    shadewire_error *err = NULL;
    shadewire_target target = {SHADEWIRE_TARGET_HLSL, 0};
    int result = 1;

    if (shadewire_mtlx_to_ir(text, len, "material.mtlx", NULL, NULL, &ir, &err) == SHADEWIRE_OK &&
        shadewire_compile(ir, target, NULL, &compiled, &err) == SHADEWIRE_OK) {
        puts(shadewire_compiled_code(compiled, NULL));
        result = 0;
    } else {
        fprintf(stderr, "error: %s\n", shadewire_error_message(err, NULL));
    }
    shadewire_compiled_free(compiled);
    shadewire_ir_free(ir);
    shadewire_error_free(err);
    return result;
}
```

The C API follows the Python module name for name:

| Python | C |
|---|---|
| `shadewire.mtlx.to_ir(text, location, material, models, working_colorspace, values_as_uniforms)` | `shadewire_mtlx_to_ir(text, len, location, material, &mtlx_options, &ir, &err)` |
| `shadewire.mtlx.material_names(text, location)` | `shadewire_mtlx_material_names(text, len, location, &mtlx_options, &strings, &err)`, then `shadewire_strings_count` / `_get` |
| `shadewire.blender.to_ir(text, material, models, working_colorspace, values_as_uniforms)` | `shadewire_blender_to_ir(text, len, material, &ir_options, &ir, &err)` (`shadewire_ir_options`: the MaterialX options without the resolver) |
| `shadewire.blender.material_names(text)` | `shadewire_blender_material_names(text, len, &strings, &err)` |
| `shadewire.mtlx.from_ir(ir, inline, name)` (text and warnings) | `shadewire_ir_to_mtlx(ir, inline_nodes, name, &doc, &err)`, then `shadewire_document_text`, `_warning_count` / `_warning`, `_free` |
| `Ir.optimize()`, `.dump()`, `.to_dict()` | `shadewire_ir_optimize(ir, &optimized, &err)`, `shadewire_ir_dump`, `shadewire_ir_json` |
| `Ir.model`, `.source`, `.structure_hash` | `shadewire_ir_model`, `shadewire_ir_source` and `_source_version`, `shadewire_ir_structure_hash` |
| `Ir.warnings` | `shadewire_ir_warnings(ir, &strings, &err)` (what the front end dropped or approximated) |
| `shadewire.wgsl.compile(ir, ...)` (and `glsl`, `hlsl`, `msl`) | `shadewire_compile(ir, target, &compile_options, &compiled, &err)` |
| `Compiled.language`, `.code`, `.reflection`, `.model_struct` | `shadewire_compiled_language`, `_code`, `_reflection_json`, `_model_struct` |
| `Compiled.ir` | `shadewire_compiled_ir_dump` (the IR the code was printed from) |
| `IR_FORMAT_VERSION`, `version()` | `SHADEWIRE_IR_FORMAT_VERSION`, `shadewire_version_string()` |

The target is `SHADEWIRE_TARGET_WGSL`, `_GLSL` (`#version` 330, 400 to 460; 0 for 420), `_GLSL_ES` (300 to 320; 0 for 300),
`_HLSL` or `_MSL`. `shadewire_compile_options` holds the back ends' keyword arguments (`group`, `first_binding`,
`reserve_uniform_binding`, `flip_v`, `optimize`, `implicit_derivatives`); `shadewire_*_options_default` fills them.
`crates/shadewire-capi-ctest` builds the same calls as a C99 (C11 on MSVC) test.

### Includes and dependencies from C

By default `xi:include`s are read from the file system, relative to the document's `location`. A host with its own
I/O (an asset database, a virtual file system, an HTTP cache) sets `resolve` in `shadewire_mtlx_options`: Shadewire
calls it with `resolve_user_data`, the including document's location and the `href`, and the callback answers with
`shadewire_resolution_set(out, location, text, len)` or `shadewire_resolution_fail(out, message)`. Both copy their
arguments, so the host's buffers stay its own. The callback runs only during the call whose options name it, on that
call's thread; locations are opaque strings (`res://…`, `mem://…`), and the location an answer gives is the base for
that document's own includes.

```c
static void resolve(void *user_data, const char *base, const char *href, shadewire_resolution *out) {
    const my_file *file = my_lookup(user_data, base, href); /* the host's own join and lookup */
    if (file != NULL) {
        shadewire_resolution_set(out, file->location, file->bytes, file->len);
    } else {
        shadewire_resolution_fail(out, "not found");
    }
}

shadewire_mtlx_options options;
shadewire_mtlx_options_default(&options);
options.resolve = resolve;
options.resolve_user_data = my_library;
shadewire_mtlx_to_ir(text, len, "res://materials/brick.mtlx", NULL, &options, &ir, &err);
```

To fetch everything up front, `shadewire_mtlx_list_dependencies(text, len, location, &deps, &err)` lists the
document's include hrefs (`shadewire_dependencies_include_count` / `_include`) and texture files with `fileprefix`
applied (`_file_count` / `_file`), unresolved and without repeats; list each fetched include again for its own, then
compile with a resolver that answers from memory. Free the list with `shadewire_dependencies_free`.

### Building a graph from C

A host that holds its own graph (a USD UsdShade network) builds the document in memory instead of writing XML for
Shadewire to parse: `shadewire_builder_new`, then nodegraphs, nodes, typed values (`shadewire_value`) and connections
(`shadewire_source`), then `shadewire_builder_to_ir`, which takes the same `shadewire_mtlx_options` and gives the same
IR as the XML would. Each call checks what it adds, and every element may carry the host's own path for it
(`source_path`), which messages show in place of a file and line. Inputs are write-once. Nodegraph and node handles
belong to the builder until `shadewire_builder_free`; a builder is not thread-safe.

```c
static const float base = 0.8f;
shadewire_builder *b = shadewire_builder_new();
shadewire_node *img = NULL, *sr = NULL;
shadewire_error *err = NULL;
shadewire_ir *ir = NULL;
shadewire_value file = {SHADEWIRE_VALUE_STRING, NULL, 0, 0, false, "wood.png"};
shadewire_value weight = {SHADEWIRE_VALUE_FLOATS, &base, 1, 0, false, NULL};
shadewire_source from_img = {SHADEWIRE_SOURCE_NODE, NULL, NULL, NULL, NULL};

if (shadewire_builder_add_node(b, NULL, "img", "ND_image_color3", "/root/Looks/Wood/img", &img, &err) ||
    shadewire_node_set_value(b, img, "file", NULL, &file, NULL, &err) ||
    shadewire_node_set_colorspace(b, img, "file", "srgb_texture", &err) ||
    shadewire_builder_add_node(b, NULL, "SR", "ND_standard_surface_surfaceshader", "/root/Looks/Wood", &sr, &err)) {
    goto done;
}
from_img.node = img;
if (shadewire_node_connect(b, sr, "base_color", NULL, &from_img, NULL, &err) ||
    shadewire_node_set_value(b, sr, "base", NULL, &weight, NULL, &err) ||
    shadewire_builder_add_material(b, "Wood", sr, "/root/Looks/Wood", &err) ||
    shadewire_builder_to_ir(b, "Wood", NULL, &ir, &err)) {
    fprintf(stderr, "error: %s\n", shadewire_error_message(err, NULL)); /* names /root/Looks/... */
}
done:
shadewire_builder_free(b); /* `ir` is independent of the builder */
```

Nodes inside a nodegraph take its `shadewire_nodegraph` handle in place of NULL
(`shadewire_builder_add_nodegraph`, `shadewire_nodegraph_add_input`, `_add_output`); a node of a category with no
nodedef named (`shadewire_builder_add_node_category`) takes explicit input types.

## Bevy

`shadewire-bevy` (Bevy 0.20) turns a MaterialX material into a `ShadewireMaterial`: a `StandardMaterial` (the base,
which keeps its alpha mode, lighting and shadows) extended with a fragment shader generated for the graph. Identical
graphs share one shader and pipeline whatever their values; a shader is freed once no material holds it.
`BuildOptions` picks the material and passes `BuildOptions.ir` (`IrOptions`: models, working colour space, `values_as_uniforms`)
through. The builder returns the material with its reflection, so uniforms can be set by name later
(`ShadewireExtension::set_uniform`). Forward rendering only; up to 8 textures per material; meshes without tangents
get a screen-space tangent frame.

```rust
use bevy::prelude::*;
use shadewire_bevy::{BuildOptions, FromMtlx, ShadewireAssets, ShadewireMaterial, ShadewirePlugin};

fn main() {
    App::new().add_plugins((DefaultPlugins, ShadewirePlugin)).add_systems(Startup, spawn).run();
}

fn spawn(mut commands: Commands, mut assets: ShadewireAssets, mut meshes: ResMut<Assets<Mesh>>,
         mut materials: ResMut<Assets<ShadewireMaterial>>) -> Result {
    let marble = ShadewireMaterial::from_mtlx_file("marble.mtlx", &BuildOptions::default(), default(), &mut assets)?;
    // marble.reflection names the uniforms: marble.material.extension.set_uniform(&marble.reflection, name, &value)
    let marble = marble.material;
    let sphere = Sphere::new(1.0).mesh().uv(64, 32).with_generated_tangents()?;
    commands.spawn((Mesh3d(meshes.add(sphere)), MeshMaterial3d(materials.add(marble))));
    Ok(())
}
```

The example puts a material on a glTF scene (every `StandardMaterial` is replaced by the MaterialX material built
over it) or, without a scene, on a sphere and a cube; drag to orbit, scroll to zoom, `R` to recompile the `.mtlx`:

```
cargo run -p shadewire-bevy --example gltf_mtlx -- [scene.glb|gltf] material.mtlx [--material NAME] [--screenshot out.png] [--lobes]
```

Coat and transmission: Bevy compiles clearcoat and specular transmission into a material only when the base
`StandardMaterial` has them above 0. When the graph's coat (`coat`, `coat_weight`, `clearcoat`) or transmission
(`transmission`, `transmission_weight`) is not 0, the builder gives a base that has them at 0 a small value (the
shader replaces it with the graph's). Specular transmission also moves the mesh into Bevy's transmissive pass. A
uniform built at 0 and raised later through `set_uniform` or the extension's `params` needs the base factor set by the app.

Closure trees: by default the builder accepts `shadewire_bevy::MODELS`, the four models it writes onto
`StandardMaterial`'s inputs and `surface`, so every other material (a raw BSDF graph, `UsdPreviewSurface`,
`disney_principled`) opens into its closure tree; `BuildOptions::lobes()` accepts only `surface` and `surface_unlit`
(`shadewire::LOBE_MODELS`), so every material does. A closure tree is lit by MaterialX's reference lobes instead of
Bevy's BRDF: the fragment walks Bevy's point, spot and directional lights, with their shadows and contact shadows,
and adds `shadewire_eval_bsdf` for each, then `shadewire_eval_emission`; the tree's opacity is the alpha. Indirect
light (lightmaps, irradiance volumes, environment maps and reflection probes, the ambient light) goes through
`shadewire_eval_ambient`: the tree's albedo split into a diffuse and a specular part takes the place of Bevy's own, so
nothing is inverted, and the specular part samples the environment at one roughness for every specular lobe. Lobes
take Bevy's light units as they are, so a white Lambert lobe lights as a white rough `StandardMaterial` under each kind
of light (`cargo run -p shadewire-bevy --example lobe_calibration` checks it). Not lit: rect lights, light textures,
the atmosphere and screen-space transmission; point and spot lights are points. The reflection's `approximations`
says so.

Limits:

- Uniform values: at most 1 KiB per material (the extension's uniform binding is `[Vec4; 64]`).
- Textures: at most 8 per material, uploaded without mipmaps. PNG and JPEG decode out of the box; EXR and HDR files
  need the app to enable Bevy's `exr` / `hdr` features.
- Coat and transmission: Bevy's clearcoat and specular transmission only, turned on in the base as above.
- Opacity: blends only over a base whose alpha mode is not `AlphaMode::Opaque` (`Blend`, `Premultiplied`, `Mask`...).
- Forward rendering only: a camera with a deferred prepass draws Shadewire meshes with Bevy's standard deferred
  shader.
- The library enables only Bevy's rendering features (`std`, `bevy_asset`, `bevy_render`, `bevy_core_pipeline`,
  `bevy_pbr`, `png`, `jpeg`); windowing, glTF and tonemapping LUTs are the app's choice.

## Python

```
pip install shadewire
```

```python
from shadewire import mtlx, glsl

text = open("material.mtlx", encoding="utf-8").read()
ir = mtlx.to_ir(text, location="material.mtlx")  # models=, resolver= for a host's own xi:include I/O
compiled = glsl.compile(ir, version=420)          # or shadewire.wgsl / .hlsl / .msl
print(compiled.code, compiled.reflection["uniforms"])
```

The module mirrors the crates: `shadewire.mtlx` (`to_ir`, `material_names`, `list_dependencies`, `from_ir`),
`shadewire.Ir` (`optimize`, `dump`, `to_dict`, `model`, `structure_hash`) and one `compile` per back end, each returning a
`Compiled` (`code`, `language`, `reflection`, `model_struct`). `mtlx.from_ir(ir, inline=False, name="material")`
writes an IR back as MaterialX and returns `(text, warnings)`. One wheel per platform serves every CPython from 3.8
(Windows x64/ARM64, macOS x64/arm64, manylinux2014 x64/aarch64). `crates/shadewire-py/examples` has complete
renderers: OpenGL with pygame (one lighting MaterialX's lobes, one with its own PBR over the named models) and
wgpu-py. See [its README](crates/shadewire-py/README.md).

## Godot

`shadewire-godot` is a Godot 4.4+ extension that compiles `.mtlx` documents to `ShaderMaterial`s in-process, at
runtime (`Shadewire.load_material`, `compile_string`, `load_material_async` for `http(s)://` sources, and the
resource loader) and when the editor imports a `.mtlx` file. Named models are mapped onto Godot's outputs and lit
by Godot's BRDF, or every material is lit by MaterialX's lobes in a generated `light()`; limits on nodes, textures,
bytes and pixels guard untrusted documents. See [its README](crates/shadewire-godot/README.md).

## Blender

The Blender extension (4.3 or later) imports MaterialX materials as Blender node trees, which Cycles and EEVEE both
render: File > Import > MaterialX, and on Blender 4.4+ a USD hook that builds the MaterialX materials of a USD scene
as it is imported (from referenced `.mtlx` files, `.usdz` included, or from the network authored in the USD).
Shading is either one Principled BSDF driven by a named model's parameters, or every material as its lobes, one
BSDF node each. The extension bundles the `shadewire` Python module; `blender/build_extension.py` builds the wheels
and the zip for extensions.blender.org. See [its README](blender/README.md).

## Reflection

Each surface field in `params` has a `kind`:

- `constant`: the value is baked into the shader and given as `value`.
- `uniform`: the field is a uniform itself (named by `uniform`), so a host may fill its parameter block from the uniform values without running the shader.
- `uniform_expr`: the field is computed from uniforms only (an authored color converted from `srgb_texture`, math over nodegraph interface values, or any op left unfolded by `--no-optimize`); its value needs the shader or the IR.
- `varying`: the field changes per fragment (texture lookups, geometry, time).

Each entry of `textures` names its file and the `colorspace` it was authored in. The shader converts every sample from that colorspace to the working one itself, so bind each texture through a raw view (for example `Rgba8Unorm`, never `Rgba8UnormSrgb`), or colors are decoded twice.

Each entry of `uniforms` also carries the MaterialX ui attributes of the input it came from, each key only when authored on the input or on its nodedef port: `uimin`, `uimax`, `uisoftmin`, `uisoftmax` (values of the uniform's type) and `uifolder` (a `/`-separated folder path). `uiname` is not carried. A limit that does not parse as the uniform's type is dropped.

In the uniform block a `boolean` uniform is a `u32` (0 or 1), since WGSL has no host-shareable `bool`; `integer` is `i32`. Offsets and sizes are in the reflection's `uniforms`.

`structure_hash` covers the material's graph only: its shape, constants, textures, geomprop defaults and ui attributes, with uniform values left out. The code also depends on its language, the bind `group`, the first binding, the V flip and whether the uniform binding is reserved, so a shader cache keys on `(structure_hash, language, group, first_binding, flip_v, reserve_uniform_binding)`.

## Languages

Bindings follow the reflection's numbers in each language's terms:

- WGSL: `@group(group) @binding(n)`, a texture and its sampler apart.
- GLSL: the uniform block at binding point `uniform_binding` and each texture (a `sampler2D`) at unit `binding`
  from GLSL 420 and ES 310; before, bind them by name (`MaterialUniforms`, `shadewire_t{index}`).
- HLSL: `register(b|t|s{binding}, space{group})`; the uniform block (a `ConstantBuffer`) padded to the reflected
  offsets.
- MSL: Metal has no global resources, so `shadewire_eval_surface` takes them as trailing parameters (the uniform
  block, then each texture and sampler); the uniform block padded to the reflected offsets.

- gdshader: a complete `shader_type spatial;` file, bound by name: uniforms keep their authored names, textures are
  `shadewire_t{index}`.

Each back end's tests compile every test material with the real compiler when it is installed: naga for WGSL,
glslangValidator for GLSL, dxc for HLSL (found on PATH, in `$VULKAN_SDK/Bin`, or as `$SHADEWIRE_GLSLANGVALIDATOR` /
`$SHADEWIRE_DXC`), `xcrun metal` for MSL (macOS with Xcode's Metal toolchain) and Godot itself for gdshader (below).
`SHADEWIRE_REQUIRE_TOOLS=1` makes a missing one a failure.

## Development

The Godot check compiles and draws every shader the Godot back end generates for the vendored examples (four ways each) and its tests with Godot's own renderer (a GPU is needed; Godot's headless renderer accepts code the real ones refuse): `python -I tools/godot_check/check_gdshader.py <path to a Godot 4.x binary>`.

The Godot extension's smoke test imports and loads materials in a Godot project and saves a screenshot: `python -I crates/shadewire-godot/tests/smoke/run.py <path to a Godot 4.4+ binary>`.

The GPU parity test (interpreter vs WGSL on a real adapter, about three minutes) is ignored by default: run it with `cargo test -p shadewire-wgsl --test gpu_parity -- --ignored`. It skips when no adapter exists; `SHADEWIRE_REQUIRE_GPU=1` makes that a failure.

The material library regression harness (`tools/matlib`) renders every 1k package (8- and 16-bit) of the
[AMD GPUOpen MaterialX Library](https://matlib.gpuopen.com) on a sphere framed like the library's previews and compares
each shot with the library's preview and with an accepted baseline. It renders with the `shadewire-pt` path tracer
(`$SHADEWIRE_PT`, lit by a generated studio environment) or, with `--renderer bevy`, the `matlib_render` Bevy example.
The data (about 6 GB) lives outside the repository, in `$SHADEWIRE_MATLIB` (default `tools/matlib/_data`, ignored by
git):

```
python -I tools/matlib/fetch.py
python -I tools/matlib/harness.py calibrate
python -I tools/matlib/harness.py run
python -I tools/matlib/harness.py accept <run dir>
```

`calibrate` (once) picks the UV scale of each material's sphere: the previews tile textures at a scale of their own.
`run` writes `report.html` in the run directory and exits 1 on a regression: a package that no longer renders, a
shot that differs from its renderer's baseline, or one further from the library's preview.
