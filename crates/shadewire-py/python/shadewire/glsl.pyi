"""The GLSL back end (the Rust crate `shadewire_glsl`): a GLSL library, desktop from 330, ES from 300."""

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
