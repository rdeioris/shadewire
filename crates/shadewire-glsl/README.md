# shadewire-glsl

The GLSL back end of [Shadewire](https://github.com/rdeioris/shadewire): a material's IR printed as a GLSL library (desktop from 330, ES from 300), with its reflection (uniforms with
their offsets and defaults, textures, the host functions the code calls, the closure tree).

The code holds the model's struct, `shadewire_eval_surface()` filling it, and for a material with lobes the functions
evaluating them as MaterialX defines (`shadewire_eval_bsdf`, `shadewire_eval_albedo`, `shadewire_eval_emission`, and
`shadewire_eval_ambient`, the albedo split into diffuse and specular for a host with its own environment lighting).
The host defines the `shadewire_host_*` functions it calls (the library declares them) and lights the result.

```rust
use shadewire::shader::CompileOptions;
use shadewire_mtlx::{MtlxOptions, to_ir};

let text = std::fs::read_to_string("material.mtlx")?;
let ir = to_ir(&text, Some("material.mtlx".as_ref()), None, &MtlxOptions::default())?;
let compiled = shadewire_glsl::compile(&ir.dag, &ir.source, &CompileOptions::default(), shadewire_glsl::Version::Desktop(420))?;
println!("{}", compiled.code);
```

From GLSL 420 and ES 310 the uniform block sits at binding point `uniform_binding` and each texture (a `sampler2D`) at unit `binding`; before, the host binds them by name.

Part of [Shadewire](https://github.com/rdeioris/shadewire), a compiler for material graphs: a front end turns a material description into a typed
IR, back ends turn it into shader code plus reflection that says what the host must bind.
