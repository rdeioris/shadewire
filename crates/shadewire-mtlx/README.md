# shadewire-mtlx

The MaterialX front end of [Shadewire](https://github.com/rdeioris/shadewire): reads MaterialX 1.38 and 1.39 documents (includes, nodegraphs,
nodedefs, colorspaces, upgrades) and lowers a material to Shadewire's IR, for one of the shading models a host
accepts: `standard_surface`, `open_pbr_surface`, `gltf_pbr`, `surface_unlit`, or `surface` (the material as a closure
tree of MaterialX's lobes, raw BSDF graphs included).

```rust
use shadewire_mtlx::{MtlxOptions, to_ir};

let text = std::fs::read_to_string("material.mtlx")?;
let ir = to_ir(&text, Some("material.mtlx".as_ref()), None, &MtlxOptions::default())?;
print!("{}", ir.dag.dump());
```

It embeds MaterialX's standard libraries (`stdlib`, `pbrlib`, `bxdf`, `cmlib` from v1.39.5, Apache-2.0: see
`materialx/LICENSE` and NOTICE).
