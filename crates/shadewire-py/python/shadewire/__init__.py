"""Shadewire: material graphs compiled to an IR and to shader code.

- `shadewire.mtlx`: the MaterialX front end (`to_ir`, `material_names`);
- `shadewire.Ir`: the IR (`optimize`, `dump`, `to_dict`);
- `shadewire.wgsl`: the WGSL back end (`compile`, `link`, `translate`, `reference_prelude`).
"""

from ._native import IR_FORMAT_VERSION, Ir, ShadewireError, mtlx, version, wgsl

__all__ = ["IR_FORMAT_VERSION", "Ir", "ShadewireError", "mtlx", "version", "wgsl"]
