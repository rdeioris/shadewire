# shadewire-mtlx

The MaterialX front end of [Shadewire](https://github.com/rdeioris/shadewire): reads MaterialX 1.38 and 1.39 documents (includes, nodegraphs,
nodedefs, colorspaces, upgrades) and lowers a material to Shadewire's IR, for one of the shading models a host
accepts: `standard_surface`, `open_pbr_surface`, `gltf_pbr`, `surface_unlit`, or `surface` (the material as a closure
tree of MaterialX's lobes, raw BSDF graphs included).

```rust
use shadewire_mtlx::{Resolver, to_ir};
use shadewire_nodes::LowerOptions;

let text = std::fs::read_to_string("material.mtlx")?;
let ir = to_ir(&text, Some("material.mtlx"), None, &LowerOptions::default(), &Resolver::default())?;
print!("{}", ir.dag.dump());
```

A document is named by a location string (an OS path, `res://…`, `https://…`) that the front end never interprets:
The `resolver` argument of `to_ir` joins and reads each `xi:include` (`Resolver::default()` reads the file system). A host whose reads are
asynchronous calls `list_dependencies(text, location)` for the `xi:include` hrefs and the texture files (after
`fileprefix`) a document names, fetches them, lists each fetched include again (includes nest), and then compiles with
a resolver that answers from memory.

It embeds MaterialX's standard libraries (`stdlib`, `pbrlib`, `bxdf`, `cmlib` from v1.39.5, Apache-2.0: see
`materialx/LICENSE` and NOTICE).
