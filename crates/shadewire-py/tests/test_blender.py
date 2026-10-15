"""`shadewire.blender`: the Blender graph fixtures lower, warn, and write as MaterialX.

Run with pytest against an installed `shadewire` (`maturin develop`), as test_mtlx.py.
"""

import json
import pathlib

import pytest

import shadewire
from shadewire import blender, mtlx

FIXTURES = pathlib.Path(__file__).resolve().parents[3] / "blender" / "fixtures" / "graphs"


def read(name):
    return (FIXTURES / f"{name}.json").read_text(encoding="utf-8")


def test_a_principled_material_is_open_pbr():
    text = read("principled_defaults")
    assert blender.material_names(text) == ["principled_defaults"]
    ir = blender.to_ir(text)
    assert ir.model == "open_pbr_surface"
    assert ir.source == ("blender", json.loads(text)["producer"])
    assert ir.warnings == []
    assert "fn shadewire_eval_surface() -> OpenPbrSurface" in shadewire.wgsl.compile(ir).code


def test_warnings_and_the_lobes_need_the_surface_model():
    ir = blender.to_ir(read("unsupported"))
    assert any("not lowered" in w for w in ir.warnings)
    with pytest.raises(shadewire.ShadewireError):
        blender.to_ir(read("lobes"))
    lobes = blender.to_ir(read("lobes"), models=["surface"])
    assert lobes.model == "surface"


def test_the_written_materialx_reads_again():
    ir = blender.to_ir(read("textured"), values_as_uniforms=False)
    text, _ = mtlx.from_ir(ir, name="textured")
    again = mtlx.to_ir(text)
    # The same material, but not the same structure hash: the texture list comes back in another order (the
    # written document's, not the order the Blender front end met the images in).
    assert again.model == ir.model
    before, after = ir.to_dict(), again.to_dict()
    assert [f["name"] for f in after["fields"]] == [f["name"] for f in before["fields"]]
    assert sorted(t["file"] for t in after["textures"]) == sorted(t["file"] for t in before["textures"])


def test_a_bad_document_is_an_error():
    with pytest.raises(shadewire.ShadewireError):
        blender.to_ir("{not json")
