# shadewire-hlsl

The HLSL back end of [Shadewire](https://github.com/rdeioris/shadewire): a material's IR printed as an HLSL library (shader model 5.1 and up), with its reflection (uniforms with
their offsets and defaults, textures, the host functions the code calls, the closure tree).

The code holds the model's struct, `shadewire_eval_surface()` filling it, and for a material with lobes the functions
evaluating them as MaterialX defines (`shadewire_eval_bsdf`, `shadewire_eval_albedo`, `shadewire_eval_emission`, and
`shadewire_eval_ambient`, the albedo split into diffuse and specular for a host with its own environment lighting).
The host defines the `shadewire_host_*` functions it calls (the library declares them) and lights the result.

```rust
use shadewire::shader::CompileOptions;
use shadewire_mtlx::{Resolver, to_ir};
use shadewire_nodes::IrOptions;

let text = std::fs::read_to_string("material.mtlx")?;
let ir = to_ir(&text, Some("material.mtlx".as_ref()), None, &IrOptions::default(), &Resolver::default())?;
let compiled = shadewire_hlsl::compile(&ir.dag, &ir.source, &CompileOptions::default())?;
println!("{}", compiled.code);
```

Resources sit at `register(b|t|s{binding}, space{group})`; the uniform block is padded to the reflected offsets.

Part of [Shadewire](https://github.com/rdeioris/shadewire), a compiler for material graphs: a front end turns a material description into a typed
IR, back ends turn it into shader code plus reflection that says what the host must bind.
