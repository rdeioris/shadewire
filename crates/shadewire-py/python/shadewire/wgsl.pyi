"""The WGSL back end (the Rust crate `shadewire_wgsl`), with naga for validation and translation."""

from typing import Any, Dict, Optional, Union

from . import Ir

class Compiled:
    @property
    def wgsl(self) -> str: ...
    @property
    def reflection(self) -> Dict[str, Any]: ...
    @property
    def model_struct(self) -> str: ...

def compile(
    ir: Ir,
    group: int = 2,
    first_binding: int = 0,
    reserve_uniform_binding: bool = False,
    flip_v: bool = True,
    optimize: bool = True,
    implicit_derivatives: bool = True,
) -> Compiled: ...
def link(generated: str, host_prelude: str) -> str: ...
def reference_prelude(compiled: Compiled) -> str: ...
def translate(
    linked: str,
    target_name: str,
    version: Optional[int] = None,
    entry_point: Optional[str] = None,
) -> Union[str, bytes]: ...
