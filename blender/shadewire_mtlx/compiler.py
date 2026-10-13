"""Compiles MaterialX materials to the Shadewire IR document.

Two backends with one interface: the `shadewire` Python module (the wheels this extension bundles, which
Blender installs; built for Python's stable ABI from 3.8, so one wheel per platform serves every Blender), and
the `shadewire` command-line compiler (`shadewire ir --json`), used when the module is missing, such as when
the add-on runs from a source checkout.
"""

import json
import os
import shutil
import subprocess
import sys

# Shadewire's closure model; `surface_unlit` stays a model (it has no lobes).
MODELS = ["surface", "surface_unlit"]
IR_FORMAT = ("shadewire-ir", 1)


class CompileError(Exception):
    pass


def checked(doc):
    if (doc.get("format"), doc.get("version")) != IR_FORMAT:
        raise CompileError(f"unexpected IR document {doc.get('format')} {doc.get('version')}")
    return doc


class ModuleBackend:
    """The `shadewire` Python module, in process."""

    def __init__(self, module):
        self.module = module
        self.name = f"shadewire module {module.version()}"

    @staticmethod
    def read(mtlx):
        try:
            with open(mtlx, encoding="utf-8") as f:
                return f.read()
        except OSError as e:
            raise CompileError(f"{mtlx}: {e}") from e

    def material_names(self, mtlx):
        try:
            return self.module.mtlx.material_names(self.read(mtlx), path=mtlx)
        except self.module.ShadewireError as e:
            raise CompileError(str(e)) from e

    def compile(self, mtlx, material, editable):
        """The MaterialX front end, the optimizer, the IR as a document."""
        try:
            ir = self.module.mtlx.to_ir(self.read(mtlx), path=mtlx, material=material, models=MODELS,
                                        values_as_uniforms=editable)
            doc = ir.optimize().to_dict()
        except self.module.ShadewireError as e:
            raise CompileError(str(e)) from e
        return checked(doc)


class ExecutableBackend:
    """The `shadewire` command-line compiler, one process per call."""

    def __init__(self, path):
        self.path = path
        self.name = f"shadewire executable {path}"

    def run(self, args):
        flags = subprocess.CREATE_NO_WINDOW if sys.platform == "win32" else 0
        done = subprocess.run([self.path, *args], capture_output=True, text=True, creationflags=flags)
        if done.returncode != 0:
            raise CompileError(done.stderr.strip() or f"shadewire exited with {done.returncode}")
        return done.stdout

    def material_names(self, mtlx):
        return [line for line in self.run(["list", mtlx]).splitlines() if line.strip()]

    def compile(self, mtlx, material, editable):
        args = ["ir", mtlx, "--json"]
        for model in MODELS:
            args += ["--model", model]
        if material:
            args += ["--material", material]
        if not editable:
            args.append("--constants")
        return checked(json.loads(self.run(args)))


def find_executable(configured=""):
    """The configured compiler, one in the add-on's `bin`, or `shadewire` on PATH; None when there is none."""
    name = "shadewire.exe" if sys.platform == "win32" else "shadewire"
    candidates = [configured, os.path.join(os.path.dirname(__file__), "bin", name), shutil.which("shadewire")]
    return next((path for path in candidates if path and os.path.isfile(path)), None)


def backend(configured_executable=""):
    """The module when it imports (and no executable is configured), else the executable."""
    if not configured_executable:
        try:
            import shadewire
        except ImportError:
            pass
        else:
            return ModuleBackend(shadewire)
    path = find_executable(configured_executable)
    if path is None:
        raise CompileError(
            "no Shadewire compiler: install the extension build that bundles the shadewire module, "
            "or set the shadewire executable in the add-on preferences"
        )
    return ExecutableBackend(path)
