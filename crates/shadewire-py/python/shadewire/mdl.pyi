"""The NVIDIA MDL front end and back end (the Rust crate `shadewire_mdl`)."""

from typing import List, Optional, Sequence

from . import Compiled, Ir

def to_ir(
    text: str,
    material: Optional[str] = None,
    models: Optional[Sequence[str]] = None,
    working_colorspace: Optional[str] = None,
    values_as_uniforms: bool = True,
    paths: Optional[Sequence[str]] = None,
    location: Optional[str] = None,
) -> Ir: ...
def material_names(text: str, paths: Optional[Sequence[str]] = None, location: Optional[str] = None) -> List[str]: ...
def compile(ir: Ir, name: str = "material", optimize: bool = True) -> Compiled: ...
