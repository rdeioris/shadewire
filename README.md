# Shadewire

Shadewire is an open-source Rust compiler from MaterialX material graphs to WGSL surface-parameter functions: it turns a material into a typed DAG, optimizes it, and emits shader code a host renderer can call.

Licensed under MIT; vendored MaterialX files are Apache-2.0 (see NOTICE). Work in progress.

## Usage

Command line:

```
shadewire compile material.mtlx --reflection material.json -o material.wgsl
shadewire compile material.mtlx --lang hlsl -o material.hlsl
```

`--lang` other than `wgsl` links the reference host (which supplies an entry point) before translating. `--constants` bakes authored values in instead of exposing them as uniforms.

Library:

```rust
use shadewire::{compile_mtlx, CompileOptions};

let text = std::fs::read_to_string("material.mtlx")?;
let out = compile_mtlx(&text, Some("material.mtlx".as_ref()), None, &CompileOptions::default())?;
println!("{}", out.wgsl);
println!("{}", out.reflection.model);
```

## Reflection

Each surface field in `params` has a `kind`:

- `constant`: the value is baked into the shader and given as `value`.
- `uniform`: the field is a uniform itself (named by `uniform`), so a host may fill its parameter block from the uniform values without running the shader.
- `uniform_expr`: the field is computed from uniforms only (an authored color converted from `srgb_texture`, math over nodegraph interface values, or any op left unfolded by `--no-optimize`); its value needs the shader or the IR.
- `varying`: the field changes per fragment (texture lookups, geometry, time).

## Targets

Other languages come from the linked WGSL module through naga. HLSL shader model 6 samplers go through naga's sampler heap (register `s0`, space 0). MSL uses `fake_missing_bindings`, so resource slots follow declaration order.

## Development

The GPU parity test (interpreter vs WGSL on a real adapter, about three minutes) is ignored by default: run it with `cargo test -p shadewire-wgsl --test gpu_parity -- --ignored`. It skips when no adapter exists; `SHADEWIRE_REQUIRE_GPU=1` makes that a failure.
