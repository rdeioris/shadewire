# shadewire-nodes

The node library of [Shadewire](https://github.com/rdeioris/shadewire): every node a material graph can use, how it
is defined, and how an instance of it becomes Shadewire's IR. It embeds MaterialX's standard libraries (`stdlib`,
`pbrlib`, `bxdf`, `cmlib` from v1.39.5, Apache-2.0: see `materialx/LICENSE` and NOTICE) and Shadewire's own
(`libraries/`, including `shadewire_ir.mtlx`: nodedefs for IR-only nodes, derivatives, hextile, gradient
samples), resolves a node to its nodedef by category and types, inlines nodegraph implementations, makes leaf
nodedefs into ops, and finishes a material as a shading model or a closure tree.

Front ends depend on it: `shadewire-mtlx` reads MaterialX documents and lowers them here; a front end for another
graph format calls `Elaborator::standalone`, `instantiate` and `finish_model`/`finish_closure` directly.

## Lowering without a document

```rust
use shadewire::Value;
use shadewire_nodes::{Arg, Elaborator, IrOptions, Synthetic};

fn lower() -> Result<(), shadewire::Error> {
    let synthetic = Synthetic::new();
    let options = IrOptions::default();
    let mut e = Elaborator::standalone(&synthetic, &options);
    let base = e.authored("base_color", Value::Color3([0.8, 0.1, 0.1]), "Principled BSDF.Base Color");
    let dag = e.finish_model("open_pbr_surface", &[("base_color", Arg::Node(base))], "Principled BSDF")?;
    Ok(())
}
```

`instantiate("overlay", Type::Color3, &[("fg", Arg::Node(a)), ("bg", Arg::Node(b))], origin)` is any library node;
the dag's `compounds` remember it was one, so a graph back end writes `overlay` again rather than its expansion.
