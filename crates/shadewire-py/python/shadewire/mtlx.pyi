"""The MaterialX front end (the Rust crate `shadewire_mtlx`)."""

from os import PathLike
from typing import List, Optional, Sequence, Union

from . import Ir

def to_ir(
    text: str,
    path: Optional[Union[str, PathLike]] = None,
    material: Optional[str] = None,
    models: Optional[Sequence[str]] = None,
    working_colorspace: Optional[str] = None,
    values_as_uniforms: bool = True,
) -> Ir: ...
def material_names(text: str, path: Optional[Union[str, PathLike]] = None) -> List[str]: ...
