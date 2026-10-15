"""The Blender material graph front end (the Rust crate `shadewire_blender`)."""

from typing import List, Optional, Sequence

from . import Ir

def to_ir(
    text: str,
    material: Optional[str] = None,
    models: Optional[Sequence[str]] = None,
    working_colorspace: Optional[str] = None,
    values_as_uniforms: bool = True,
) -> Ir: ...
def material_names(text: str) -> List[str]: ...
