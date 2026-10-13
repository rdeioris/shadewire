# shadewire

Compile MaterialX materials to shader code from Python: WGSL, GLSL (desktop and ES), HLSL and Metal, each with the
reflection a renderer needs to bind them. The compiler is Rust ([Shadewire](https://github.com/rdeioris/shadewire)); one
wheel per platform serves every CPython from 3.8.

```
pip install shadewire
```

```python
from shadewire import mtlx, glsl

text = open("material.mtlx", encoding="utf-8").read()
ir = mtlx.to_ir(text, location="material.mtlx")      # the material in Shadewire's IR
compiled = glsl.compile(ir, version=420)         # or shadewire.wgsl / .hlsl / .msl
print(compiled.code)                             # the model's struct, shadewire_eval_surface(), ...
print(compiled.reflection["uniforms"])           # offsets and defaults of the uniform block
print(compiled.reflection["textures"])           # files, colorspaces, bindings
```

- `shadewire.mtlx`: `to_ir(text, location=None, material=None, models=None, ...)` and `material_names(text, location=None)`.
  `models` lists the shading models the renderer accepts: `standard_surface`, `open_pbr_surface`, `gltf_pbr`,
  `surface_unlit`, and `surface`, the material as a closure tree of MaterialX's lobes (raw BSDF graphs included).
  `resolver=` reads `xi:include`s your way: a callable `(base, href) -> (location, text)` that raises to fail (None
  reads the file system relative to `location`). `list_dependencies(text, location=None)` returns a document's
  `{"includes": [...], "files": [...]}`, so a host with its own I/O can fetch everything first:

  ```python
  def resolve(base, href):                         # locations are opaque strings: paths, URLs, archive members
      location = base.rsplit("/", 1)[0] + "/" + href
      return location, fetched[location]           # a KeyError fails the compile with its message

  ir = mtlx.to_ir(text, location="https://example.com/m.mtlx", resolver=resolve)
  ```
- `shadewire.Ir`: `optimize()`, `dump()`, `to_dict()` (the IR as a JSON-like document), `model`, `structure_hash`.
- `shadewire.wgsl`, `shadewire.glsl`, `shadewire.hlsl`, `shadewire.msl`: `compile(ir, ...)` returns a
  `shadewire.Compiled` with `code`, `language`, `reflection` and `model_struct`.

The generated code calls host functions (`shadewire_host_normal_world()`, `shadewire_host_texcoord_0()`...) that the
renderer defines next to its lights; for a material with lobes it also evaluates them as MaterialX defines
(`shadewire_eval_bsdf`, `shadewire_eval_albedo`, `shadewire_eval_emission`). The repository's
`crates/shadewire-py/examples` has complete renderers: OpenGL with pygame (a host lighting MaterialX's lobes, and one
with its own PBR over the named models) and wgpu-py.

Licensed MIT; parts port MaterialX (Apache-2.0) and Open Shading Language (BSD-3-Clause): see NOTICE.
