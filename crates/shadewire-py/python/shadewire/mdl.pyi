"""The NVIDIA MDL front end and back end (the Rust crate `shadewire_mdl`)."""

from typing import Dict, List, Optional, Sequence, Tuple, Union

from . import Compiled, Ir

# A material argument: bool, int (or an enum's value), float, 2-4 floats, str, or
# {"texture": path, "colorspace": "sRGB" | "raw" | "auto"}.
MdlArgValue = Union[bool, int, float, Sequence[float], str, Dict[str, Optional[str]]]

def to_ir(
    text: str,
    material: Optional[str] = None,
    models: Optional[Sequence[str]] = None,
    working_colorspace: Optional[str] = None,
    values_as_uniforms: bool = True,
    paths: Optional[Sequence[str]] = None,
    location: Optional[str] = None,
    distill: bool = False,
    arguments: Optional[Sequence[Tuple[str, MdlArgValue]]] = None,
) -> Ir: ...
def material_names(text: str, paths: Optional[Sequence[str]] = None, location: Optional[str] = None) -> List[str]: ...
def compile(ir: Ir, name: str = "material", optimize: bool = True) -> Compiled: ...
