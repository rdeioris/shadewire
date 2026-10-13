"""The WGSL back end (the Rust crate `shadewire_wgsl`)."""

from . import Compiled, Ir

def compile(
    ir: Ir,
    group: int = 2,
    first_binding: int = 0,
    reserve_uniform_binding: bool = False,
    flip_v: bool = True,
    optimize: bool = True,
    implicit_derivatives: bool = True,
) -> Compiled: ...
