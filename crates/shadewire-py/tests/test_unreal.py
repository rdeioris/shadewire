"""`shadewire.unreal`: the Unreal graph fixtures lower, warn, and write as MaterialX and MDL.

Run with pytest against an installed `shadewire` (`maturin develop`), as test_mtlx.py.
"""

import json
import pathlib

import pytest

import shadewire
from shadewire import mtlx, unreal

FIXTURES = pathlib.Path(__file__).resolve().parents[3] / "unreal" / "fixtures" / "graphs"


def read(name):
    return (FIXTURES / f"{name}.json").read_text(encoding="utf-8")


def test_a_default_lit_material_is_open_pbr():
    text = read("M_Constants")
    assert unreal.material_names(text) == ["M_Constants"]
    ir = unreal.to_ir(text)
    assert ir.model == "open_pbr_surface"
    assert ir.source == ("unreal", json.loads(text)["producer"])
    assert ir.warnings == []
    assert "fn shadewire_eval_surface() -> OpenPbrSurface" in shadewire.wgsl.compile(ir).code


def test_an_instance_chain_sets_its_parameters():
    ir = unreal.to_ir(read("MI_ParametersChild"))
    uniforms = {n["name"]: n["value"] for n in ir.to_dict()["nodes"] if n["kind"] == "uniform"}
    assert uniforms["Metallic"] == pytest.approx([0.0])
    assert uniforms["Roughness"] == pytest.approx([0.25])
    assert uniforms["Tint"] == pytest.approx([0.1, 0.6, 0.2, 1.0])


def test_warnings_and_mdl_as_closure_trees():
    ir = unreal.to_ir(read("M_Geometry"))
    assert any("Custom HLSL" in w for w in ir.warnings)
    lobes = unreal.to_ir(read("M_Textured"), models=["surface", "surface_unlit"])
    assert "export material" in shadewire.mdl.compile(lobes, name="M_Textured").code


def test_the_written_materialx_reads_again():
    ir = unreal.to_ir(read("M_Textured"), values_as_uniforms=False)
    text, _ = mtlx.from_ir(ir, name="M_Textured")
    again = mtlx.to_ir(text)
    assert again.model == ir.model
    assert sorted(t["file"] for t in again.to_dict()["textures"]) == sorted(t["file"] for t in ir.to_dict()["textures"])


def test_a_bad_document_is_an_error():
    with pytest.raises(shadewire.ShadewireError):
        unreal.to_ir("{not json")
