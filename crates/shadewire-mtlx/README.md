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

## Writing

```rust
use shadewire_mtlx::{IrOptions, Resolver, WriteOptions, from_ir, to_ir};

let text = std::fs::read_to_string("material.mtlx")?;
let ir = to_ir(&text, Some("material.mtlx"), None, &IrOptions::default(), &Resolver::default())?;
let written = from_ir(&ir.dag, &WriteOptions::default())?;
std::fs::write("out.mtlx", written.text)?;
```

`from_ir` writes any IR as a MaterialX 1.39 document. Library nodes are written as one node each, from the IR's
compounds, unless `WriteOptions::inline` expands them. Uniforms are nodegraph inputs and constants are constants, so a
round trip keeps the IR's shape. IR-only ops (derivatives, hextile, gradient samples) are written as Shadewire's own
nodedefs, whose declarations are embedded in the document and listed in `written.warnings`. `from_ir` takes the IR as
`to_ir` returns it, unoptimized: the optimizer drops the compounds and the image colorspace conversions the writer
folds, so an optimized dag writes as leaf ops and may convert a colour texture twice.

The standard libraries ship in `shadewire-nodes` (`crates/shadewire-nodes/materialx`, Apache-2.0: see its
`materialx/LICENSE` and NOTICE); this crate's `upgrade` module ports the upstream 1.38 to 1.39 upgrade rules
(Apache-2.0, see NOTICE).
