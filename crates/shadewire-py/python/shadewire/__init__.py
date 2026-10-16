"""Shadewire: material graphs compiled to an IR and to shader code.

- `shadewire.mtlx`: the MaterialX front end (`to_ir`, `material_names`, `list_dependencies`);
- `shadewire.blender`: the Blender graph front end (`to_ir`, `material_names`);
- `shadewire.mdl`: the MDL front end (`to_ir`, `material_names`) and back end (`compile`);
- `shadewire.Ir`: the IR (`optimize`, `dump`, `to_dict`, `warnings`);
- `shadewire.wgsl`, `shadewire.glsl`, `shadewire.hlsl`, `shadewire.msl`: the back ends, each a `compile` to a
  `shadewire.Compiled` (the code and its reflection).
"""

from ._native import (
    IR_FORMAT_VERSION,
    Compiled,
    Ir,
    ShadewireError,
    blender,
    glsl,
    hlsl,
    mdl,
    msl,
    mtlx,
    version,
    wgsl,
)

__all__ = [
    "IR_FORMAT_VERSION",
    "Compiled",
    "Ir",
    "ShadewireError",
    "blender",
    "glsl",
    "hlsl",
    "mdl",
    "msl",
    "mtlx",
    "version",
    "wgsl",
]
