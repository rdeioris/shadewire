"""The HLSL back end (the Rust crate `shadewire_hlsl`): an HLSL library, shader model 5.1 and up."""

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
