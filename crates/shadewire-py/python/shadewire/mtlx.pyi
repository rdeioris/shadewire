"""The MaterialX front end (the Rust crate `shadewire_mtlx`)."""

from typing import Callable, Dict, List, Optional, Sequence, Tuple

from . import Ir

# `resolver(base, href) -> (location, text)`: an include's location (the base of its own includes) and its text.
# Raise to fail; the exception's message becomes the `ShadewireError`'s.
Resolver = Callable[[str, str], Tuple[str, str]]

def to_ir(
    text: str,
    location: Optional[str] = None,
    material: Optional[str] = None,
    models: Optional[Sequence[str]] = None,
    working_colorspace: Optional[str] = None,
    values_as_uniforms: bool = True,
    resolver: Optional[Resolver] = None,
) -> Ir: ...
def material_names(
    text: str, location: Optional[str] = None, resolver: Optional[Resolver] = None
) -> List[str]: ...
def list_dependencies(text: str, location: Optional[str] = None) -> Dict[str, List[str]]: ...
# The IR (unoptimized, as `to_ir` returns it) as a MaterialX document: `(text, warnings)`. `name` names the
# `surfacematerial`; `inline` writes library nodes as their expansions.
def from_ir(ir: Ir, inline: bool = False, name: str = "material") -> Tuple[str, List[str]]: ...
