"""Shadewire: MaterialX materials compiled to the Shadewire IR (see `shadewire ir --json`)."""

from os import PathLike
from typing import Any, Dict, List, Optional, Sequence, Union

IR_FORMAT_VERSION: int

class ShadewireError(Exception):
    """A MaterialX document Shadewire cannot read or compile."""

def version() -> str: ...
def material_names(path: Union[str, PathLike]) -> List[str]: ...
def compile_ir(
    path: Union[str, PathLike],
    material: Optional[str] = None,
    models: Optional[Sequence[str]] = None,
    constants: bool = False,
    optimize: bool = True,
) -> Dict[str, Any]: ...
def compile_ir_text(
    text: str,
    path: Optional[Union[str, PathLike]] = None,
    material: Optional[str] = None,
    models: Optional[Sequence[str]] = None,
    constants: bool = False,
    optimize: bool = True,
) -> Dict[str, Any]: ...
