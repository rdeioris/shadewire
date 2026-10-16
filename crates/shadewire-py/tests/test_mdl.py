"""`shadewire.mdl`: MDL modules lower to the IR, and the IR writes as an MDL module.

Run with pytest against an installed `shadewire` (`maturin develop`), as test_mtlx.py.
"""

import pathlib

import pytest

import shadewire
from shadewire import mdl, mtlx

FIXTURES = pathlib.Path(__file__).resolve().parents[2] / "shadewire-mdl" / "tests" / "fixtures"
LOBES = ["surface", "surface_unlit"]


def read(name):
    return (FIXTURES / f"{name}.mdl").read_text(encoding="utf-8")


def test_material_names_and_a_lowered_material():
    text = read("mixes")
    assert mdl.material_names(text) == ["normalized", "clamped", "unbounded", "color_normalized", "weighted", "tinted"]
    ir = mdl.to_ir(text, material="weighted", models=LOBES)
    assert ir.model == "surface"
    assert ir.source == ("mdl", "MDL 1.7")
    uniforms = [n for n in ir.to_dict()["nodes"] if n["kind"] == "uniform"]
    assert [(u["name"], u["value"]) for u in uniforms] == [("w", [0.4])]
    assert ir.warnings == []


def test_default_models_refuse_an_mdl_material():
    with pytest.raises(shadewire.ShadewireError, match="no translation"):
        mdl.to_ir(read("diffuse"), material="reflection")


def test_search_paths_resolve_imports(tmp_path):
    (tmp_path / "lib").mkdir()
    (tmp_path / "lib" / "tints.mdl").write_text("mdl 1.7;\nexport color red() = color(1.0, 0.0, 0.0);\n")
    text = (
        "mdl 1.7;\nimport ::df::*;\nimport ::lib::tints::*;\n"
        "export material m() = material(surface: material_surface(scattering: "
        "df::diffuse_reflection_bsdf(tint: lib::tints::red())));\n"
    )
    ir = mdl.to_ir(text, models=LOBES, paths=[str(tmp_path)])
    assert ir.model == "surface"
    with pytest.raises(shadewire.ShadewireError):
        mdl.to_ir(text, models=LOBES)


def test_a_materialx_material_writes_as_mdl():
    porcelain = (pathlib.Path(__file__).resolve().parents[2] / "shadewire-mtlx" / "tests" / "fixtures" / "porcelain.mtlx")
    ir = mtlx.to_ir(porcelain.read_text(encoding="utf-8"), models=LOBES)
    compiled = mdl.compile(ir, name="porcelain")
    assert compiled.language == "mdl"
    assert compiled.code.startswith("mdl 1.7;")
    assert "export material porcelain(" in compiled.code
    again = mdl.to_ir(compiled.code, models=LOBES)
    assert again.model == "surface"
