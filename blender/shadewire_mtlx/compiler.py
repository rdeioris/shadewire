"""Runs the shadewire compiler: a MaterialX material to its IR document (`shadewire ir --json`)."""

import json
import os
import shutil
import subprocess
import sys

# Shadewire's closure model; `surface_unlit` stays a model (it has no lobes).
MODELS = ("surface", "surface_unlit")


class CompileError(Exception):
    pass


def executable(configured=""):
    """The compiler: the configured path, one bundled in the add-on's `bin`, or `shadewire` on PATH."""
    name = "shadewire.exe" if sys.platform == "win32" else "shadewire"
    candidates = []
    if configured:
        candidates.append(configured)
    candidates.append(os.path.join(os.path.dirname(__file__), "bin", name))
    found = shutil.which("shadewire")
    if found:
        candidates.append(found)
    for path in candidates:
        if path and os.path.isfile(path):
            return path
    raise CompileError(
        "shadewire not found: set its path in the add-on preferences, put it in the add-on's bin folder, "
        "or on PATH (cargo build -p shadewire-cli --release)"
    )


def _run(exe, args):
    flags = subprocess.CREATE_NO_WINDOW if sys.platform == "win32" else 0
    done = subprocess.run([exe, *args], capture_output=True, text=True, creationflags=flags)
    if done.returncode != 0:
        raise CompileError(done.stderr.strip() or f"shadewire exited with {done.returncode}")
    return done.stdout


def material_names(exe, mtlx):
    return [line for line in _run(exe, ["list", mtlx]).splitlines() if line.strip()]


def compile_material(exe, mtlx, material, editable):
    """The IR document of `material` (None: the document's only one). `editable` keeps authored values as
    uniforms (Value/RGB nodes); otherwise they are constants and the closure tree is pruned by them."""
    args = ["ir", mtlx, "--json"]
    for model in MODELS:
        args += ["--model", model]
    if material:
        args += ["--material", material]
    if not editable:
        args.append("--constants")
    doc = json.loads(_run(exe, args))
    if doc.get("format") != "shadewire-ir" or doc.get("version") != 1:
        raise CompileError(f"unexpected IR document {doc.get('format')} {doc.get('version')}")
    return doc
