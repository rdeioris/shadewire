# SPDX-FileCopyrightText: 2026 Roberto De Ioris
# SPDX-License-Identifier: GPL-3.0-or-later

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
import tempfile
import xml.etree.ElementTree as ET

# The models each import mode accepts. PRINCIPLED keeps the named models (one Principled BSDF each) and takes the
# rest, raw BSDF graphs included, as Shadewire's closure model; LOBES opens every material into its lobes.
# `surface_unlit` stays a model either way (it has no lobes).
MODES = {
    "PRINCIPLED": ["standard_surface", "open_pbr_surface", "gltf_pbr", "surface_unlit", "surface"],
    "LOBES": ["surface", "surface_unlit"],
}
# The models an export lowers to: a Principled BSDF stays one, lobes become a closure tree.
EXPORT_MODELS = ["open_pbr_surface", "surface", "surface_unlit"]
# The element tags Shadewire embeds in every document it writes, which repeat when documents are merged.
EMBEDDED_TAGS = ("nodedef", "implementation")
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
            return self.module.mtlx.material_names(self.read(mtlx), location=mtlx)
        except self.module.ShadewireError as e:
            raise CompileError(str(e)) from e

    def compile(self, mtlx, material, editable, mode="PRINCIPLED"):
        """The MaterialX front end, the optimizer, the IR as a document."""
        return self.compile_text(self.read(mtlx), mtlx, material, editable, mode)

    def compile_text(self, text, path, material, editable, mode="PRINCIPLED"):
        """`compile` of a document's text; `path` (None for none) anchors its includes."""
        try:
            ir = self.module.mtlx.to_ir(text, location=path, material=material, models=MODES[mode],
                                        values_as_uniforms=editable)
            doc = ir.optimize().to_dict()
        except self.module.ShadewireError as e:
            raise CompileError(str(e)) from e
        return checked(doc)

    def to_mtlx(self, graph_text, material, name, inline=False):
        """A Blender graph document's `material` as a MaterialX document: `(text, warnings)`. `name` names the
        written `surfacematerial`; `inline` writes library nodes expanded."""
        if not hasattr(self.module, "blender"):
            raise CompileError(missing_front_end(self.module))
        try:
            ir = self.module.blender.to_ir(graph_text, material=material, models=EXPORT_MODELS,
                                           values_as_uniforms=True)
            text, writer_warnings = self.module.mtlx.from_ir(ir, inline=inline, name=name)
        except self.module.ShadewireError as e:
            raise CompileError(str(e)) from e
        return text, list(ir.warnings) + list(writer_warnings)


class ExecutableBackend:
    """The `shadewire` command-line compiler, one process per call."""

    def __init__(self, path):
        self.path = path
        self.name = f"shadewire executable {path}"

    def run(self, args):
        return self.run_with_stderr(args)[0]

    def run_with_stderr(self, args):
        """`(stdout, stderr)` of a successful call; a failed one is a CompileError carrying stderr."""
        flags = subprocess.CREATE_NO_WINDOW if sys.platform == "win32" else 0
        done = subprocess.run([self.path, *args], capture_output=True, text=True, encoding="utf-8", errors="replace",
                              creationflags=flags)
        if done.returncode != 0:
            raise CompileError(done.stderr.strip() or f"shadewire exited with {done.returncode}")
        return done.stdout, done.stderr

    def material_names(self, mtlx):
        return [line for line in self.run(["list", mtlx]).splitlines() if line.strip()]

    def compile(self, mtlx, material, editable, mode="PRINCIPLED"):
        args = ["ir", mtlx, "--json"]
        for model in MODES[mode]:
            args += ["--model", model]
        if material:
            args += ["--material", material]
        if not editable:
            args.append("--constants")
        return checked(json.loads(self.run(args)))

    def compile_text(self, text, path, material, editable, mode="PRINCIPLED"):
        """`compile` of a document's text, through a file next to `path` (its includes resolve from there) or in
        the temporary folder."""
        folder = os.path.dirname(path) if path and os.path.isdir(os.path.dirname(path)) else None
        fd, temporary = tempfile.mkstemp(suffix=".mtlx", dir=folder)
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as f:
                f.write(text)
            return self.compile(temporary, material, editable, mode)
        finally:
            os.remove(temporary)

    def to_mtlx(self, graph_text, material, name, inline=False):
        """`ModuleBackend.to_mtlx` through the `mtlx` command: the warnings are stderr's lines. The command names
        the written material after the one it reads, so the document handed over holds just `material`, renamed
        `name`."""
        renamed = renamed_material(graph_text, material, name)
        fd, temporary = tempfile.mkstemp(suffix=".json")
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as f:
                f.write(renamed)
            args = ["mtlx", temporary, "--format", "blender", "--material", name]
            for model in EXPORT_MODELS:
                args += ["--model", model]
            if inline:
                args.append("--inline")
            text, stderr = self.run_with_stderr(args)
        except CompileError as e:
            raise CompileError(self.hinted(str(e))) from e
        finally:
            os.remove(temporary)
        return text, [line.strip() for line in stderr.splitlines() if line.strip()]

    def hinted(self, message):
        """`message`, with what to do about it when the executable predates the Blender front end: clap's words
        for a command, an argument or a `--format` value it does not know."""
        lowered = message.lower()
        old = ("unexpected argument", "unrecognized subcommand", "unknown format")
        if any(phrase in lowered for phrase in old) or ("invalid value" in lowered and "--format" in lowered):
            return (f"{message}\nthe shadewire executable at {self.path} does not read Blender graphs: "
                    "a build with the Blender front end (0.1.0 or later) is needed")
        return message


def renamed_material(graph_text, material, name):
    """The graph document (its groups kept) with only `material`, under the name `name`."""
    try:
        doc = json.loads(graph_text)
        chosen = [dict(entry, name=name) for entry in doc["materials"] if entry["name"] == material]
    except (ValueError, KeyError, TypeError) as e:
        raise CompileError(f"not a Blender graph document: {e}") from e
    if not chosen:
        raise CompileError(f"material {material} is not in the Blender graph document")
    return json.dumps(dict(doc, materials=chosen))


def merge_documents(texts):
    """One MaterialX document holding every element of `texts` in order. Shadewire's embedded declarations
    (`nodedef`, `implementation`, the `nodegraph` of a nodedef) repeat per document and are kept once; any other
    repeated name is an error (the same material name twice, or two names the writer sanitizes alike); a repeated
    declaration is dropped even if its content differs. An element without a name is always kept. A single
    document is returned as it is."""
    merged = MergedDocument()
    for text in texts:
        merged.add(text)
    return merged.text()


class MergedDocument:
    """MaterialX documents merged into one as `merge_documents` merges them, one document at a time."""

    def __init__(self):
        self.root = ET.Element("materialx", version="1.39")
        self.taken = set()
        self.texts = []

    def add(self, text):
        """Adds the elements of `text`; a document that repeats a name already taken (but for an embedded
        declaration) or does not parse is a CompileError, and nothing of it is added."""
        try:
            root = ET.fromstring(text)
        except ET.ParseError as e:
            raise CompileError(f"not a MaterialX document: {e}") from e
        added, taken = [], set(self.taken)
        for element in root:
            name = element.get("name")
            if name in taken and is_embedded(element):
                continue
            if name in taken:
                raise CompileError(f"two exported materials write an element named {name}; rename one material")
            if name is not None:
                taken.add(name)
            added.append(element)
        self.taken = taken
        self.root.extend(added)
        self.texts.append(text)

    def text(self):
        """The merged document; a single document as it was given."""
        if len(self.texts) == 1:
            return self.texts[0]
        ET.indent(self.root, space="  ")
        return '<?xml version="1.0"?>\n' + ET.tostring(self.root, encoding="unicode") + "\n"


def sanitize_name(text):
    """The name the MaterialX writer gives `text`: ASCII letters, digits and `_` kept, every other character `_`, a
    leading digit prefixed with `_`; an empty text is `node`."""
    name = "".join(c if c.isascii() and (c.isalnum() or c == "_") else "_" for c in text) or "node"
    return "_" + name if name[0].isdigit() else name


def unique_name(text, taken):
    """`sanitize_name(text)`, suffixed `_2`, `_3`, ... until it is not in `taken`."""
    base = name = sanitize_name(text)
    n = 2
    while name in taken:
        name = f"{base}_{n}"
        n += 1
    return name


def is_embedded(element):
    """Whether `element` is one of the declarations every Shadewire document embeds."""
    return element.tag in EMBEDDED_TAGS or (element.tag == "nodegraph" and element.get("nodedef") is not None)


def find_executable(configured=""):
    """The configured compiler, one in the add-on's `bin`, or `shadewire` on PATH; None when there is none."""
    name = "shadewire.exe" if sys.platform == "win32" else "shadewire"
    candidates = [configured, os.path.join(os.path.dirname(__file__), "bin", name), shutil.which("shadewire")]
    return next((path for path in candidates if path and os.path.isfile(path)), None)


def missing_front_end(module):
    """The error for an installed `module` that predates the Blender front end."""
    return (f"the installed shadewire module {module.version()} has no Blender front end; this extension needs its "
            "bundled module (0.1.0 with shadewire.blender) or the shadewire executable")


def backend(configured_executable=""):
    """The module when it imports with its Blender front end (and no executable is configured), else the
    executable. A module without the front end counts as absent."""
    old = None
    if not configured_executable:
        try:
            import shadewire
        except ImportError:
            pass
        else:
            if hasattr(shadewire, "blender"):
                return ModuleBackend(shadewire)
            old = shadewire
    path = find_executable(configured_executable)
    if path is None and old is not None:
        raise CompileError(missing_front_end(old))
    if path is None:
        raise CompileError(
            "no Shadewire compiler: install the extension build that bundles the shadewire module, "
            "or set the shadewire executable in the add-on preferences"
        )
    return ExecutableBackend(path)
