"""`shadewire.mtlx` includes through a Python resolver, and `list_dependencies`.

Run with pytest (or `python tests/test_mtlx.py`) against an installed `shadewire`, e.g. after `maturin develop`.
"""

import pytest

import shadewire
from shadewire import mtlx

# The material at top level, its surface in lib/surface.mtlx, the BSDF in lib/bsdf.mtlx (included by surface.mtlx).
DOCUMENTS = {
    "mem://top.mtlx": """<materialx version="1.39">
  <xi:include href="lib/surface.mtlx"/>
  <surfacematerial name="m" type="material"><input name="surfaceshader" type="surfaceshader" nodename="s"/></surfacematerial>
</materialx>""",
    "mem://lib/surface.mtlx": """<materialx version="1.39">
  <xi:include href="bsdf.mtlx"/>
  <surface name="s" type="surfaceshader"><input name="bsdf" type="BSDF" nodename="d"/></surface>
</materialx>""",
    "mem://lib/bsdf.mtlx": """<materialx version="1.39">
  <oren_nayar_diffuse_bsdf name="d" type="BSDF"/>
</materialx>""",
}


def join(base, href):
    """`href` next to `base`: locations here are URLs, not OS paths."""
    return base.rsplit("/", 1)[0] + "/" + href


class MemoryResolver:
    """Serves DOCUMENTS, joining `href` to the directory of `base`; records each call."""

    def __init__(self):
        self.calls = []

    def __call__(self, base, href):
        self.calls.append((base, href))
        location = join(base, href)
        if location not in DOCUMENTS:
            raise FileNotFoundError(f"no document at {location}")
        return location, DOCUMENTS[location]


def test_resolver_serves_nested_includes():
    resolver = MemoryResolver()
    ir = mtlx.to_ir(DOCUMENTS["mem://top.mtlx"], location="mem://top.mtlx", models=["surface"], resolver=resolver)
    assert ir.to_dict()["closure"]["nodes"][0]["kind"] == "oren_nayar_diffuse_bsdf"
    # Each include resolves against its own document's location.
    assert resolver.calls == [("mem://top.mtlx", "lib/surface.mtlx"), ("mem://lib/surface.mtlx", "bsdf.mtlx")]
    assert "fn shadewire_eval_surface" in shadewire.wgsl.compile(ir).code


def test_material_names_through_the_resolver():
    names = mtlx.material_names(DOCUMENTS["mem://lib/surface.mtlx"], "mem://lib/surface.mtlx", MemoryResolver())
    assert names == []
    assert mtlx.material_names(DOCUMENTS["mem://top.mtlx"], location="mem://top.mtlx", resolver=MemoryResolver()) == [
        "m"
    ]


def test_a_raising_resolver_fails_with_its_message():
    def refuse(base, href):
        raise PermissionError(f"refusing {href} from {base}")

    with pytest.raises(shadewire.ShadewireError, match="refusing lib/surface.mtlx from mem://top.mtlx"):
        mtlx.to_ir(DOCUMENTS["mem://top.mtlx"], location="mem://top.mtlx", resolver=refuse)
    with pytest.raises(shadewire.ShadewireError, match="refusing"):
        mtlx.material_names(DOCUMENTS["mem://top.mtlx"], location="mem://top.mtlx", resolver=refuse)


def test_a_missing_nested_include_fails():
    documents = dict(DOCUMENTS)
    del documents["mem://lib/bsdf.mtlx"]

    def resolve(base, href):
        location = join(base, href)
        return location, documents[location]

    with pytest.raises(shadewire.ShadewireError, match="mem://lib/bsdf.mtlx"):
        mtlx.to_ir(DOCUMENTS["mem://top.mtlx"], location="mem://top.mtlx", resolver=resolve)


def test_a_resolver_answering_the_wrong_type_fails():
    with pytest.raises(shadewire.ShadewireError):
        mtlx.to_ir(DOCUMENTS["mem://top.mtlx"], location="mem://top.mtlx", resolver=lambda base, href: None)


def test_list_dependencies():
    text = """<materialx version="1.39" fileprefix="tex/">
  <xi:include href="lib/a.mtlx"/>
  <xi:include href="lib/b.mtlx"/>
  <xi:include href="lib/a.mtlx"/>
  <image name="base" type="color3"><input name="file" type="filename" value="base.png"/></image>
  <nodegraph name="g" fileprefix="maps/">
    <image name="n" type="vector3"><input name="file" type="filename" value="normal.png"/></image>
    <image name="again" type="color3"><input name="file" type="filename" value="normal.png"/></image>
  </nodegraph>
</materialx>"""
    deps = mtlx.list_dependencies(text, location="mem://top.mtlx")
    assert deps == {"includes": ["lib/a.mtlx", "lib/b.mtlx"], "files": ["tex/base.png", "maps/normal.png"]}
    assert mtlx.list_dependencies(DOCUMENTS["mem://lib/bsdf.mtlx"]) == {"includes": [], "files": []}
    with pytest.raises(shadewire.ShadewireError):
        mtlx.list_dependencies("<materialx", location="mem://broken.mtlx")


if __name__ == "__main__":
    raise SystemExit(pytest.main([__file__, "-v"]))
