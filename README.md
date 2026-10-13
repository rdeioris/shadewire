# Shadewire

Shadewire is an open-source compiler for material graphs. A **front end** turns a material
description into Shadewire's IR, a typed DAG that computes the material's surface parameters; a
**back end** turns the IR into shader code a host renderer calls, plus reflection that says what
the host must bind. The host keeps its own lighting (BRDFs); Shadewire only fills the parameters of
a shading model (`open_pbr_surface`, `standard_surface`, `gltf_pbr`, `surface_unlit`).

Licensed under MIT; vendored MaterialX files are Apache-2.0 (see NOTICE). The Blender add-on
(`blender/shadewire_mtlx`) is GPL-3.0-or-later, as extensions.blender.org requires. Work in progress.

## Crates

Front ends and back ends never depend on each other; they share only the core, and the caller
(your code, the CLI, the C API) composes them.

| Crate | Role | Depends on |
|---|---|---|
| `shadewire` | core: the IR, optimizer, CPU interpreter, reflection, errors; the shader tree every back end prints (`shader`: the material's code, the support libraries, the shared printing) | — |
| `shadewire-mtlx` | front end: MaterialX 1.38/1.39 → IR (`to_ir`) | `shadewire` |
| `shadewire-wgsl` | back end: IR → WGSL + reflection (`compile`) | `shadewire` |
| `shadewire-glsl` | back end: IR → a GLSL library (desktop from 330, ES from 300) + reflection | `shadewire` |
| `shadewire-gdshader` | back end: IR → a complete Godot 4 spatial shader (`.gdshader`) + reflection: named models mapped onto Godot's outputs, or closure trees lit by MaterialX's reference lobes in `light()` | `shadewire` |
| `shadewire-hlsl` | back end: IR → an HLSL library (shader model 5.1 and up) + reflection | `shadewire` |
| `shadewire-msl` | back end: IR → a Metal Shading Language library + reflection | `shadewire` |
| `shadewire-cli` | the `shadewire` command | all of the above |
| `shadewire-capi` | C API (`shadewire.h`): MaterialX → code in any back end's language + reflection | `shadewire`, `shadewire-mtlx`, the back ends |
| `shadewire-bevy` | Bevy 0.20: MaterialX → a `StandardMaterial` extension with a generated fragment shader (`ShadewirePlugin`, `FromMtlx`) | `shadewire`, `shadewire-mtlx`, `shadewire-wgsl`, `bevy` |
| `shadewire-godot` | Godot 4.4+ extension (gdext; its own workspace, Rust 1.94): `.mtlx` → `ShaderMaterial` at runtime (`Shadewire`, `load()`) and on import | `shadewire`, `shadewire-mtlx`, `shadewire-gdshader`, `godot` |

Each back end is a printer of the shared shader tree: everything they have in common (the material's code, the
support libraries, operator precedence, declaration order, identifiers valid in every language) lives in the core.
Planned: a glTF front end, more back ends.

## What to depend on

| You want | Crates | Features |
|---|---|---|
| Inspect or print a material's IR | `shadewire`, `shadewire-mtlx` | — |
| Evaluate a material on the CPU (tests, baking, thumbnails) | `shadewire`, `shadewire-mtlx` | — |
| MaterialX → WGSL + reflection (wgpu, Bevy, WebGPU) | `shadewire-mtlx`, `shadewire-wgsl` | — |
| MaterialX → a Godot 4 shader (for a `ShaderMaterial`) | `shadewire-mtlx`, `shadewire-gdshader` | — |
| MaterialX materials in a Godot project or game | the `shadewire-godot` addon ([its README](crates/shadewire-godot/README.md)) | — |
| MaterialX → GLSL, GLSL ES, HLSL or MSL | `shadewire-mtlx`, `shadewire-glsl` / `-hlsl` / `-msl` | — |
| Your own IR (any source) → any of them | `shadewire` and the back end | — |
| A command-line tool | `shadewire-cli` (the `shadewire` binary) | — |
| A C/C++ host | `shadewire-capi` (shared or static library + `shadewire.h`) | — |
| A Bevy app | `shadewire-bevy` | — |

## Usage

Library, two steps: front end, then back end.

```rust
use shadewire::optimize::optimize;
use shadewire_mtlx::{MtlxOptions, to_ir};
use shadewire_wgsl::{CompileOptions, compile};

let text = std::fs::read_to_string("material.mtlx")?;
let ir = to_ir(&text, Some("material.mtlx"), None, &MtlxOptions::default())?;
print!("{}", optimize(&ir.dag).dump()); // the IR, one node per line

let out = compile(&ir.dag, &ir.source, &CompileOptions::default())?;
println!("{}", out.code);             // fn shadewire_eval_surface() -> <ModelStruct>
println!("{}", out.reflection.model);
```

Every back end writes the same library in its language: the model's struct, the uniform block and textures,
`shadewire_eval_surface()`, and for a material with lobes the functions evaluating them as MaterialX defines
(`shadewire_eval_bsdf`, `shadewire_eval_albedo`, `shadewire_eval_emission`). The code calls host functions
(`shadewire_host_texcoord_0()`, `shadewire_host_normal_world()`, ...), declared in GLSL, HLSL and MSL, which the host
defines in its own code next to its lights and entry point. Examples:
`cargo run -p shadewire-mtlx --example mtlx_to_wgsl -- material.mtlx` and `--example mtlx_to_hlsl`.

Command line:

```
shadewire compile material.mtlx --reflection material.json -o material.wgsl
shadewire compile material.mtlx --lang hlsl -o material.hlsl
shadewire compile material.mtlx --lang gdshader -o material.gdshader
shadewire ir material.mtlx               # print the IR (optimized; --no-optimize for the raw graph)
```

- `--material NAME` picks the material when the document has several.
- `--model MODEL` (repeatable) names the surface models the host accepts; by default all four. A material of another model is an error, not a translation.
- `--lang gdshader` writes a complete Godot 4 `shader_type spatial;` file. Godot binds by name: uniforms keep their authored names, made valid identifiers clear of reserved and generated ones (`GdshaderOutput::uniform_names`); textures are `shadewire_t<index>`, bound raw since the shader converts colorspaces. By default named models (`open_pbr_surface`, `standard_surface`, `gltf_pbr`, `UsdPreviewSurface`, `disney_principled`, `surface_unlit`) are mapped onto Godot's outputs and lit by Godot's BRDF, and a pbrlib `surface` is lit by MaterialX's reference lobes in a generated `light()`; `--lobes` lights every material that way, named models opened into closure trees (ambient light and reflections then see the closure's albedo as diffuse). What Godot cannot represent is listed in the reflection's `approximations`. `--transparency auto|opaque|scissor|hash|blend` (default `auto`: opaque for a constant opacity of 1, scissor for a constant glTF MASK or USD threshold, blend otherwise) and `--emission-scale` (default 1) apply to it only.
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
    shadewire_compiled *compiled = NULL;
    shadewire_error *err = NULL;
    shadewire_target target = {SHADEWIRE_TARGET_HLSL, 0};
    int result = 1;

    if (shadewire_compile_mtlx(text, len, "material.mtlx", NULL, NULL, target, NULL, &compiled, &err) == SHADEWIRE_OK) {
        puts(shadewire_compiled_code(compiled, NULL));
        result = 0;
    } else {
        fprintf(stderr, "error: %s\n", shadewire_error_message(err, NULL));
    }
    shadewire_compiled_free(compiled);
    shadewire_error_free(err);
    return result;
}
```

`shadewire_compiled_code` and `shadewire_compiled_reflection_json` read the code and the reflection;
`shadewire_compile` compiles an IR handle from `shadewire_mtlx_to_ir`. The target is `SHADEWIRE_TARGET_WGSL`,
`_GLSL` and `_GLSL_ES` (with their `#version`, 0 for the default), `_HLSL` or `_MSL`. `crates/shadewire-capi-ctest`
builds the same calls as a C99 (C11 on MSVC) test.

## Bevy

`shadewire-bevy` (Bevy 0.20) turns a MaterialX material into a `ShadewireMaterial`: a `StandardMaterial` (the base,
which keeps its alpha mode, lighting and shadows) extended with a fragment shader generated for the graph. Identical
graphs share one shader and pipeline whatever their values. Forward rendering only; up to 8 textures per material;
meshes without tangents get a screen-space tangent frame.

```rust
use bevy::prelude::*;
use shadewire_bevy::{FromMtlx, ShadewireMaterial, ShadewirePlugin};

fn main() {
    App::new().add_plugins((DefaultPlugins, ShadewirePlugin)).add_systems(Startup, spawn).run();
}

fn spawn(mut commands: Commands, mut images: ResMut<Assets<Image>>, mut shaders: ResMut<Assets<Shader>>,
         mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<ShadewireMaterial>>) -> Result {
    let marble = ShadewireMaterial::from_mtlx_file("marble.mtlx", None, default(), &mut images, &mut shaders)?;
    let sphere = Sphere::new(1.0).mesh().uv(64, 32).with_generated_tangents()?;
    commands.spawn((Mesh3d(meshes.add(sphere)), MeshMaterial3d(materials.add(marble))));
    Ok(())
}
```

The example puts a material on a glTF scene (every `StandardMaterial` is replaced by the MaterialX material built
over it) or, without a scene, on a sphere and a cube; drag to orbit, scroll to zoom, `R` to recompile the `.mtlx`:

```
cargo run -p shadewire-bevy --example gltf_mtlx -- [scene.glb|gltf] material.mtlx [--material NAME] [--screenshot out.png]
```

Coat and transmission: Bevy compiles clearcoat and specular transmission into a material only when the base
`StandardMaterial` has them above 0. When the graph's coat (`coat`, `coat_weight`, `clearcoat`) or transmission
(`transmission`, `transmission_weight`) is not 0, the builder gives a base that has them at 0 a small value (the
shader replaces it with the graph's). Specular transmission also moves the mesh into Bevy's transmissive pass. A
uniform built at 0 and raised later through the extension's `params` needs the base factor set by the app.

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

## Reflection

Each surface field in `params` has a `kind`:

- `constant`: the value is baked into the shader and given as `value`.
- `uniform`: the field is a uniform itself (named by `uniform`), so a host may fill its parameter block from the uniform values without running the shader.
- `uniform_expr`: the field is computed from uniforms only (an authored color converted from `srgb_texture`, math over nodegraph interface values, or any op left unfolded by `--no-optimize`); its value needs the shader or the IR.
- `varying`: the field changes per fragment (texture lookups, geometry, time).

Each entry of `textures` names its file and the `colorspace` it was authored in. The shader converts every sample from that colorspace to the working one itself, so bind each texture through a raw view (for example `Rgba8Unorm`, never `Rgba8UnormSrgb`), or colors are decoded twice.

In the uniform block a `boolean` uniform is a `u32` (0 or 1), since WGSL has no host-shareable `bool`; `integer` is `i32`. Offsets and sizes are in the reflection's `uniforms`.

`structure_hash` covers the material's graph only: its shape, constants, textures and geomprop defaults, with uniform values left out. The code also depends on its language, the bind `group`, the first binding, the V flip and whether the uniform binding is reserved, so a shader cache keys on `(structure_hash, language, group, first_binding, flip_v, reserve_uniform_binding)`.

## Languages

Bindings follow the reflection's numbers in each language's terms:

- WGSL: `@group(group) @binding(n)`, a texture and its sampler apart.
- GLSL: the uniform block at binding point `uniform_binding` and each texture (a `sampler2D`) at unit `binding`
  from GLSL 420 and ES 310; before, bind them by name (`MaterialUniforms`, `shadewire_t{index}`).
- HLSL: `register(b|t|s{binding}, space{group})`; the uniform block (a `ConstantBuffer`) padded to the reflected
  offsets.
- MSL: Metal has no global resources, so `shadewire_eval_surface` takes them as trailing parameters (the uniform
  block, then each texture and sampler); the uniform block padded to the reflected offsets.

Each back end's tests compile every test material with the real compiler when it is installed: naga for WGSL,
glslangValidator for GLSL, dxc for HLSL (found on PATH, in `$VULKAN_SDK/Bin`, or as `$SHADEWIRE_GLSLANGVALIDATOR` /
`$SHADEWIRE_DXC`; `SHADEWIRE_REQUIRE_TOOLS=1` makes a missing one a failure). MSL has no such check yet.

## Development

The Godot check compiles and draws every shader the Godot back end generates for the vendored examples (four ways each) and its tests with Godot's own renderer (a GPU is needed; Godot's headless renderer accepts code the real ones refuse): `python -I tools/godot_check/check_gdshader.py <path to a Godot 4.x binary>`.

The Godot extension's smoke test imports and loads materials in a Godot project and saves a screenshot: `python -I crates/shadewire-godot/tests/smoke/run.py <path to a Godot 4.4+ binary>`.

The GPU parity test (interpreter vs WGSL on a real adapter, about three minutes) is ignored by default: run it with `cargo test -p shadewire-wgsl --test gpu_parity -- --ignored`. It skips when no adapter exists; `SHADEWIRE_REQUIRE_GPU=1` makes that a failure.
