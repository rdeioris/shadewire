from typing import Any, Dict, Tuple

from . import glsl as glsl
from . import hlsl as hlsl
from . import msl as msl
from . import mtlx as mtlx
from . import wgsl as wgsl

IR_FORMAT_VERSION: int

class ShadewireError(Exception):
    """A document Shadewire cannot read or compile."""

class Ir:
    """A material in Shadewire's IR: a typed DAG computing a shading model's fields, and for the `surface`
    model the closure tree over them. Made by a front end (`shadewire.mtlx.to_ir`), compiled by a back end
    (`shadewire.wgsl.compile`, `shadewire.glsl.compile`, ...)."""

    @property
    def model(self) -> str: ...
    @property
    def source(self) -> Tuple[str, str]: ...
    @property
    def structure_hash(self) -> str: ...
    def optimize(self) -> "Ir": ...
    def dump(self) -> str: ...
    def to_dict(self) -> Dict[str, Any]: ...

class Compiled:
    """A compiled material: the code in its language (the model's struct, `shadewire_eval_surface()`, the
    closure functions, declarations of the host functions it calls) and its reflection."""

    @property
    def language(self) -> str: ...
    @property
    def code(self) -> str: ...
    @property
    def reflection(self) -> Dict[str, Any]: ...
    @property
    def model_struct(self) -> str: ...

def version() -> str: ...
