from typing import Any, Dict, List, Tuple

from . import blender as blender
from . import glsl as glsl
from . import hlsl as hlsl
from . import mdl as mdl
from . import msl as msl
from . import mtlx as mtlx
from . import wgsl as wgsl

IR_FORMAT_VERSION: int

class ShadewireError(Exception):
    """A document Shadewire cannot read or compile."""

class Ir:
    """A material in Shadewire's IR: a typed DAG computing a shading model's fields, and for the `surface`
    model the closure tree over them. Made by a front end (`shadewire.mtlx.to_ir`, `shadewire.blender.to_ir`),
    compiled by a back end (`shadewire.wgsl.compile`, `shadewire.glsl.compile`, ...)."""

    @property
    def model(self) -> str: ...
    @property
    def source(self) -> Tuple[str, str]: ...
    @property
    def warnings(self) -> List[str]: ...
    @property
    def structure_hash(self) -> str: ...
    def optimize(self) -> "Ir": ...
    def dump(self) -> str: ...
    def to_dict(self) -> Dict[str, Any]: ...

class Compiled:
    """A compiled material: the code in its language (the model's struct, `shadewire_eval_surface()`, the
    closure functions, declarations of the host functions it calls), its reflection, and the IR the code was
    printed from (optimized when the back end was asked to)."""

    @property
    def language(self) -> str: ...
    @property
    def code(self) -> str: ...
    @property
    def reflection(self) -> Dict[str, Any]: ...
    @property
    def model_struct(self) -> str: ...
    @property
    def ir(self) -> Ir: ...

def version() -> str: ...
