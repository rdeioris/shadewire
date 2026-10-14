# shadewire-mtlx

The MaterialX front end of [Shadewire](https://github.com/rdeioris/shadewire): reads MaterialX 1.38 and 1.39 documents (includes, nodegraphs,
nodedefs, colorspaces, upgrades) and lowers a material to Shadewire's IR, for one of the shading models a host
accepts: `standard_surface`, `open_pbr_surface`, `gltf_pbr`, `surface_unlit`, or `surface` (the material as a closure
tree of MaterialX's lobes, raw BSDF graphs included).

```rust
use shadewire_mtlx::{IrOptions, Resolver, to_ir};

let text = std::fs::read_to_string("material.mtlx")?;
let ir = to_ir(&text, Some("material.mtlx"), None, &IrOptions::default(), &Resolver::default())?;
print!("{}", ir.dag.dump());
```

A document is named by a location string (an OS path, `res://…`, `https://…`) that the front end never interprets:
The `resolver` argument of `to_ir` joins and reads each `xi:include` (`Resolver::default()` reads the file system). A host whose reads are
asynchronous calls `list_dependencies(text, location)` for the `xi:include` hrefs and the texture files (after
`fileprefix`) a document names, fetches them, lists each fetched include again (includes nest), and then compiles with
a resolver that answers from memory.

The standard libraries ship in `shadewire-nodes` (`crates/shadewire-nodes/materialx`, Apache-2.0: see its
`materialx/LICENSE` and NOTICE); this crate's `upgrade` module ports the upstream 1.38 to 1.39 upgrade rules
(Apache-2.0, see NOTICE).
