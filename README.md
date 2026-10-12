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

Planned: a glTF front end, a C API (`shadewire-capi`), more back ends.

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
println!("{}", out.wgsl);             // fn sw_eval_surface() -> <ModelStruct>
println!("{}", out.reflection.model);
```

The generated code calls host functions (`mx_texcoord_0()`, `mx_normal_world()`, ...) that the
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

## Reflection

Each surface field in `params` has a `kind`:

- `constant`: the value is baked into the shader and given as `value`.
- `uniform`: the field is a uniform itself (named by `uniform`), so a host may fill its parameter block from the uniform values without running the shader.
- `uniform_expr`: the field is computed from uniforms only (an authored color converted from `srgb_texture`, math over nodegraph interface values, or any op left unfolded by `--no-optimize`); its value needs the shader or the IR.
- `varying`: the field changes per fragment (texture lookups, geometry, time).

Each entry of `textures` names its file and the `colorspace` it was authored in. The shader converts every sample from that colorspace to the working one itself, so bind each texture through a raw view (for example `Rgba8Unorm`, never `Rgba8UnormSrgb`), or colors are decoded twice.

In the uniform block a `boolean` uniform is a `u32` (0 or 1), since WGSL has no host-shareable `bool`; `integer` is `i32`. Offsets and sizes are in the reflection's `uniforms`.

`structure_hash` covers the material's graph only (uniform values left out). The WGSL also depends on the bind `group`, the first binding and the V flip, so a shader cache keys on `(structure_hash, group, first_binding, flip_v)`.

## Targets

The WGSL back end writes the other languages from the linked WGSL module through naga, behind its `translate` feature (on by default; `default-features = false` leaves naga out). Targets are Shadewire's own types (`HlslShaderModel`, `GlslVersion`, `MslVersion`), so callers never depend on naga. HLSL shader model 6 samplers go through naga's sampler heap (register `s0`, space 0). MSL uses `fake_missing_bindings`, so resource slots follow declaration order.

## Development

The GPU parity test (interpreter vs WGSL on a real adapter, about three minutes) is ignored by default: run it with `cargo test -p shadewire-wgsl --test gpu_parity -- --ignored`. It skips when no adapter exists; `SHADEWIRE_REQUIRE_GPU=1` makes that a failure.
