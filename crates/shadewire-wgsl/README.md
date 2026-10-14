# shadewire-wgsl

The WGSL back end of [Shadewire](https://github.com/rdeioris/shadewire): a material's IR printed as WGSL (wgpu, WebGPU, Bevy), with its reflection (uniforms with
their offsets and defaults, textures, the host functions the code calls, the closure tree).

The code holds the model's struct, `shadewire_eval_surface()` filling it, and for a material with lobes the functions
evaluating them as MaterialX defines (`shadewire_eval_bsdf`, `shadewire_eval_albedo`, `shadewire_eval_emission`).
The host defines the `shadewire_host_*` functions it calls and lights the result.

```rust
use shadewire::shader::CompileOptions;
use shadewire_mtlx::{Resolver, to_ir};
use shadewire_nodes::LowerOptions;

let text = std::fs::read_to_string("material.mtlx")?;
let ir = to_ir(&text, Some("material.mtlx".as_ref()), None, &LowerOptions::default(), &Resolver::default())?;
let compiled = shadewire_wgsl::compile(&ir.dag, &ir.source, &CompileOptions::default())?;
println!("{}", compiled.code);
```

Resources sit at `@group(group) @binding(n)` as the reflection gives them; WGSL has no declarations, so the host's definitions of the host functions join the module.

Part of [Shadewire](https://github.com/rdeioris/shadewire), a compiler for material graphs: a front end turns a material description into a typed
IR, back ends turn it into shader code plus reflection that says what the host must bind.
