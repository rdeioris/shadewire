# shadewire

The core of [Shadewire](https://github.com/rdeioris/shadewire): the material IR (a typed DAG computing a shading model's parameters, with the
closure tree of lobes for the `surface` model), its optimizer, a CPU interpreter, reflection, and `shader`, the typed
shader tree every back end prints (the material's code, the support libraries ported from MaterialX, and the printing
the languages share).

Front ends (`shadewire-mtlx`) produce the IR; back ends (`shadewire-wgsl`, `shadewire-glsl`, `shadewire-hlsl`,
`shadewire-msl`) print it. Use this crate directly to build an IR from another source, to evaluate a material on the
CPU, or to write a back end.

Licensed MIT; the support libraries port MaterialX (Apache-2.0) and its noise and color code derives from Open Shading
Language (BSD-3-Clause): see NOTICE.
