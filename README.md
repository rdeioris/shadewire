# Shadewire

Shadewire is an open-source Rust compiler from MaterialX material graphs to WGSL surface-parameter functions: it turns a material into a typed DAG, optimizes it, and emits shader code a host renderer can call.

Licensed under MIT; vendored MaterialX files are Apache-2.0 (see NOTICE). Work in progress.

## Usage

Command line:

```
shadewire compile material.mtlx --reflection material.json -o material.wgsl
shadewire compile material.mtlx --lang hlsl -o material.hlsl
shadewire ir material.mtlx               # print the IR (optimized; --no-optimize for the raw graph)
```

Options:

- `--material NAME` picks the material when the document has several.
- `--model MODEL` (repeatable) names the surface models the host accepts; by default all four (`open_pbr_surface`, `standard_surface`, `gltf_pbr`, `surface_unlit`). A material of another model is an error, not a translation.
- `--lang wgsl|hlsl|glsl|glsl-es|msl|spirv`: anything but `wgsl` links the reference host (which supplies an entry point) before translating; SPIR-V needs `-o`.
- `--reflection FILE` writes the reflection JSON; it is written only when the shader was produced.
- `--no-optimize` skips constant folding, simplification and sharing.
- `--constants` bakes authored values in instead of exposing them as uniforms.

`structure_hash` covers the material's graph only (uniform values left out). The WGSL also depends on the bind `group`, the first binding and the V flip, so a shader cache keys on `(structure_hash, group, first_binding, flip_v)`.

In the uniform block a `boolean` uniform is a `u32` (0 or 1), since WGSL has no host-shareable `bool`; `integer` is `i32`. Offsets and sizes are in the reflection's `uniforms`.

Library — the crates:

- `shadewire-ir`: the IR (a typed DAG), optimizer, CPU interpreter, reflection.
- `shadewire-backend`: IR to WGSL and reflection (`compile`); with its `translate` feature (on by default), naga translation to HLSL, GLSL, MSL and SPIR-V.
- `shadewire-mtlx`: the MaterialX front end, MaterialX to IR (`to_ir`) and MaterialX to WGSL in one call (`compile_mtlx`). It never pulls in naga.
- `shadewire-cli`: the `shadewire` command.

```rust
use shadewire_backend::{CompileOptions, compile};
use shadewire_ir::optimize::optimize;
use shadewire_mtlx::{MtlxOptions, to_ir};

let text = std::fs::read_to_string("material.mtlx")?;
let ir = to_ir(&text, Some("material.mtlx".as_ref()), None, &MtlxOptions::default())?;
print!("{}", optimize(&ir.dag).dump()); // the IR, one node per line

let out = compile(&ir.dag, &ir.source, &CompileOptions::default())?;
println!("{}", out.wgsl);
println!("{}", out.reflection.model);
```

`shadewire_mtlx::compile_mtlx(text, file, material, &MtlxOptions, &CompileOptions)` does both steps in one call. Examples: `cargo run -p shadewire-mtlx --example mtlx_to_wgsl -- material.mtlx` and `--example mtlx_to_hlsl`.

## Reflection

Each surface field in `params` has a `kind`:

- `constant`: the value is baked into the shader and given as `value`.
- `uniform`: the field is a uniform itself (named by `uniform`), so a host may fill its parameter block from the uniform values without running the shader.
- `uniform_expr`: the field is computed from uniforms only (an authored color converted from `srgb_texture`, math over nodegraph interface values, or any op left unfolded by `--no-optimize`); its value needs the shader or the IR.
- `varying`: the field changes per fragment (texture lookups, geometry, time).

Each entry of `textures` names its file and the `colorspace` it was authored in. The shader converts every sample from that colorspace to the working one itself, so bind each texture through a raw view (for example `Rgba8Unorm`, never `Rgba8UnormSrgb`), or colors are decoded twice.

## Targets

Other languages come from the linked WGSL module through naga, behind the `translate` feature of `shadewire-backend` (on by default; `default-features = false` gives a naga-free back end). Targets are Shadewire's own types (`HlslShaderModel`, `GlslVersion`, `MslVersion`), so callers never depend on naga. HLSL shader model 6 samplers go through naga's sampler heap (register `s0`, space 0). MSL uses `fake_missing_bindings`, so resource slots follow declaration order.

## Development

The GPU parity test (interpreter vs WGSL on a real adapter, about three minutes) is ignored by default: run it with `cargo test -p shadewire-backend --test gpu_parity -- --ignored`. It skips when no adapter exists; `SHADEWIRE_REQUIRE_GPU=1` makes that a failure.
