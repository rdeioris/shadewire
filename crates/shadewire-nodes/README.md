# shadewire-nodes

The node library of [Shadewire](https://github.com/rdeioris/shadewire): every node a material graph can use, how it
is defined, and how an instance of it becomes Shadewire's IR. It embeds MaterialX's standard libraries (`stdlib`,
`pbrlib`, `bxdf`, `cmlib` from v1.39.5, Apache-2.0: see `materialx/LICENSE` and NOTICE) and Shadewire's own
(`libraries/`), resolves a node to its nodedef by category and types, inlines nodegraph implementations, makes
leaf nodedefs into ops, and finishes a material as a shading model or a closure tree.

Front ends depend on it: `shadewire-mtlx` reads MaterialX documents and lowers them here; a front end for another
graph format calls `Elaborator::standalone`, `instantiate` and `finish_model`/`finish_closure` directly.
