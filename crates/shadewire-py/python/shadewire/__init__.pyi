from typing import Any, Dict, Tuple

from . import mtlx as mtlx
from . import wgsl as wgsl

IR_FORMAT_VERSION: int

class ShadewireError(Exception):
    """A document Shadewire cannot read, compile or link."""

class Ir:
    """A material in Shadewire's IR: a typed DAG computing a shading model's fields, and for the `surface`
    model the closure tree over them. Made by a front end (`shadewire.mtlx.to_ir`), compiled by a back end
    (`shadewire.wgsl.compile`)."""

    @property
    def model(self) -> str: ...
    @property
    def source(self) -> Tuple[str, str]: ...
    @property
    def structure_hash(self) -> str: ...
    def optimize(self) -> "Ir": ...
    def dump(self) -> str: ...
    def to_dict(self) -> Dict[str, Any]: ...

def version() -> str: ...
