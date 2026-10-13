# Shadewire

Shadewire is an open-source compiler for material graphs. A **front end** turns a material
description into Shadewire's IR, a typed DAG that computes the material's surface parameters; a
**back end** turns the IR into shader code a host renderer calls, plus reflection that says what
the host must bind. The host keeps its own lighting (BRDFs); Shadewire only fills the parameters of
a shading model (`open_pbr_surface`, `standard_surface`, `gltf_pbr`, `surface_unlit`).

Licensed under MIT; vendored MaterialX files are Apache-2.0 (see NOTICE). Work in progress.

## Crates

Front ends and back ends never depend on each other; they share only the core, and the caller
(your code, the CLI, the C API) composes them.

| Crate | Role | Depends on |
|---|---|---|
| `shadewire` | core: the IR, optimizer, CPU interpreter, reflection, errors | — |
| `shadewire-mtlx` | front end: MaterialX 1.38/1.39 → IR (`to_ir`) | `shadewire` |
| `shadewire-wgsl` | back end: IR → WGSL + reflection (`compile`); feature `translate` (default): naga translation of the linked module to HLSL, GLSL, MSL, SPIR-V | `shadewire` (+ naga) |
| `shadewire-cli` | the `shadewire` command | all of the above |
| `shadewire-capi` | C API (`shadewire.h`): MaterialX → WGSL + reflection, translation to the other languages | `shadewire`, `shadewire-mtlx`, `shadewire-wgsl` |
| `shadewire-bevy` | Bevy 0.20: MaterialX → a `StandardMaterial` extension with a generated fragment shader (`ShadewirePlugin`, `FromMtlx`) | `shadewire`, `shadewire-mtlx`, `shadewire-wgsl` (no naga), `bevy` |

Planned: a glTF front end, more back ends.

## What to depend on

| You want | Crates | Features |
|---|---|---|
| Inspect or print a material's IR | `shadewire`, `shadewire-mtlx` | — |
| Evaluate a material on the CPU (tests, baking, thumbnails) | `shadewire`, `shadewire-mtlx` | — |
| MaterialX → WGSL + reflection (wgpu, Bevy, WebGPU) | `shadewire-mtlx`, `shadewire-wgsl` | `shadewire-wgsl`: `default-features = false` (no naga) |
| MaterialX → HLSL, GLSL, GLSL ES, MSL or SPIR-V | `shadewire-mtlx`, `shadewire-wgsl` | `shadewire-wgsl`: `translate` (default) |
| Your own IR (any source) → WGSL | `shadewire`, `shadewire-wgsl` | `default-features = false` |
| Your own IR → HLSL/GLSL/MSL/SPIR-V | `shadewire`, `shadewire-wgsl` | `translate` (default) |
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
let ir = to_ir(&text, Some("material.mtlx".as_ref()), None, &MtlxOptions::default())?;
print!("{}", optimize(&ir.dag).dump()); // the IR, one node per line

let out = compile(&ir.dag, &ir.source, &CompileOptions::default())?;
println!("{}", out.wgsl);             // fn shadewire_eval_surface() -> <ModelStruct>
println!("{}", out.reflection.model);
```

The generated code calls host functions (`shadewire_host_texcoord_0()`, `shadewire_host_normal_world()`, ...) that the
host defines in its own WGSL prelude, next to its BRDF and entry point; `link` joins and validates
the two, `translate` writes the other languages. Examples:
`cargo run -p shadewire-mtlx --example mtlx_to_wgsl -- material.mtlx` and `--example mtlx_to_hlsl`.

Command line:

```
shadewire compile material.mtlx --reflection material.json -o material.wgsl
shadewire compile material.mtlx --lang hlsl -o material.hlsl
shadewire ir material.mtlx               # print the IR (optimized; --no-optimize for the raw graph)
```

- `--material NAME` picks the material when the document has several.
- `--model MODEL` (repeatable) names the surface models the host accepts; by default all four. A material of another model is an error, not a translation.
- `--lang wgsl|hlsl|glsl|glsl-es|msl|spirv`: anything but `wgsl` links the reference host (which supplies an entry point) before translating; SPIR-V needs `-o`.
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

/* Compiles `len` bytes of MaterialX `text` and prints it as HLSL shader model 6.0; returns 0 on success. */
int print_hlsl(const char *text, size_t len) {
    shadewire_compiled *compiled = NULL;
    shadewire_blob *hlsl = NULL;
    shadewire_error *err = NULL;
    shadewire_target target = {SHADEWIRE_TARGET_HLSL, 60};
    int result = 1;

    if (shadewire_compile_mtlx(text, len, "material.mtlx", NULL, NULL, NULL, &compiled, &err) == SHADEWIRE_OK &&
        shadewire_translate(compiled, NULL, target, &hlsl, &err) == SHADEWIRE_OK) {
        puts((const char *)shadewire_blob_data(hlsl, NULL)); /* text blobs are NUL-terminated */
        result = 0;
    } else {
        fprintf(stderr, "error: %s\n", shadewire_error_message(err, NULL));
    }
    shadewire_blob_free(hlsl);
    shadewire_compiled_free(compiled);
    shadewire_error_free(err);
    return result;
}
```

`shadewire_compiled_wgsl` and `shadewire_compiled_reflection_json` read the WGSL and the reflection; a NULL host
prelude in `shadewire_translate` links the reference host. `crates/shadewire-capi-ctest` builds the same calls as a
C99 (C11 on MSVC) test.

## Bevy

`shadewire-bevy` (Bevy 0.20) turns a MaterialX material into a `ShadewireMaterial`: a `StandardMaterial` (the base,
which keeps its alpha mode, lighting and shadows) extended with a fragment shader generated for the graph. Identical
graphs share one shader and pipeline whatever their values; a shader is freed once no material holds it.
`BuildOptions` picks the material and passes `MtlxOptions` (models, working colour space, `values_as_uniforms`)
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
cargo run -p shadewire-bevy --example gltf_mtlx -- [scene.glb|gltf] material.mtlx [--material NAME] [--screenshot out.png]
```

Coat and transmission: Bevy compiles clearcoat and specular transmission into a material only when the base
`StandardMaterial` has them above 0. When the graph's coat (`coat`, `coat_weight`, `clearcoat`) or transmission
(`transmission`, `transmission_weight`) is not 0, the builder gives a base that has them at 0 a small value (the
shader replaces it with the graph's). Specular transmission also moves the mesh into Bevy's transmissive pass. A
uniform built at 0 and raised later through `set_uniform` or the extension's `params` needs the base factor set by the app.

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

`structure_hash` covers the material's graph only: its shape, constants, textures and geomprop defaults, with uniform values left out. The WGSL also depends on the bind `group`, the first binding, the V flip and whether the uniform binding is reserved, so a shader cache keys on `(structure_hash, group, first_binding, flip_v, reserve_uniform_binding)`.

## Targets

The WGSL back end writes the other languages from the linked WGSL module through naga, behind its `translate` feature (on by default; `default-features = false` leaves naga out). Targets are Shadewire's own types (`HlslShaderModel`, `GlslVersion`, `MslVersion`), so callers never depend on naga. HLSL shader model 6 samplers go through naga's sampler heap (register `s0`, space 0). MSL uses `fake_missing_bindings`, so resource slots follow declaration order.

## Development

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
