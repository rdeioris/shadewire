"""The GLSL back end (the Rust crate `shadewire_glsl`): a GLSL library, desktop 330 and 400 to
460 (default 420), ES 300 to 320 (default 300). Another version raises `ShadewireError`."""

from typing import Optional

from . import Compiled, Ir

def compile(
    ir: Ir,
    version: Optional[int] = None,
    es: bool = False,
    group: int = 2,
    first_binding: int = 0,
    reserve_uniform_binding: bool = False,
    flip_v: bool = True,
    optimize: bool = True,
    implicit_derivatives: bool = True,
) -> Compiled: ...
