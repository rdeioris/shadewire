"""Bake oracle values with MaterialX's own GLSL (the TextureBaker's renderer) for shadewire's tests.

Run: python -I tools/oracle/bake_oracle.py  (needs `pip install materialx==1.39.5`, a GL context)

The TextureBaker writes float bakes as .hdr when MaterialX is built without
OpenImageIO (the PyPI wheel), and RGBE keeps only 8 bits of mantissa. So this
script runs the baker's own steps (generate the "BakingShader" for each graph
output feeding the surface, createProgram, renderTextureSpace, captureImage)
with the generator context bakeMaterialToDoc sets up, and reads the float
RGBA32F capture in memory instead of a file.
"""
import json
import pathlib
import sys

import MaterialX as mx
import MaterialX.PyMaterialXGenGlsl as mx_gen_glsl
import MaterialX.PyMaterialXGenShader as mx_gen_shader
import MaterialX.PyMaterialXRender as mx_render
import MaterialX.PyMaterialXRenderGlsl as mx_render_glsl

ROOT = pathlib.Path(__file__).resolve().parents[2]
MATERIALS = ROOT / "tools/oracle/materials"
OUT = ROOT / "crates/shadewire/tests/oracle"
SIZE = 16


def search_path():
    search = mx.getDefaultDataSearchPath()
    search.append(mx.FilePath(str(ROOT / "vendor/materialx")))
    return search


def load(path, search):
    doc = mx.createDocument()
    libraries = mx.createDocument()
    mx.loadLibraries(mx.getDefaultDataLibraryFolders(), search, libraries)
    mx.readFromXmlFile(doc, str(path), search)
    doc.importLibrary(libraries)
    valid, message = doc.validate()
    if not valid:
        raise RuntimeError(f"{path.name}: {message}")
    return doc


def context_for(doc, generator, search):
    """The generator context TextureBaker::bakeMaterialToDoc builds."""
    context = mx_gen_shader.GenContext(generator)
    context.getOptions().targetColorSpaceOverride = "lin_rec709"
    context.getOptions().fileTextureVerticalFlip = True
    cms = mx_gen_shader.DefaultColorManagementSystem.create(generator.getTarget())
    cms.loadLibrary(doc)
    context.registerSourceCodeSearchPath(search)
    generator.setColorManagementSystem(cms)
    return context


def shader_node(doc):
    materials = [n for n in doc.getMaterialNodes()]
    if len(materials) != 1:
        raise RuntimeError("one material per oracle document")
    shaders = mx.getShaderNodes(materials[0])
    return shaders[0]


def bake(baker, path, search):
    doc = load(path, search)
    generator = mx_gen_glsl.GlslShaderGenerator.create()
    context = context_for(doc, generator, search)
    baker.getImageHandler().setSearchPath(search)
    fields = {}
    for shader_input in shader_node(doc).getInputs():
        output = shader_input.getConnectedOutput()
        if output is None:
            continue
        shader = generator.generate("BakingShader", output, context)
        baker.createProgram(shader)
        baker.renderTextureSpace(baker.getTextureSpaceMin(), baker.getTextureSpaceMax())
        image = baker.captureImage(None)
        if image.getWidth() != SIZE or image.getHeight() != SIZE:
            raise RuntimeError("unexpected capture size")
        texels = []
        for y in range(SIZE):
            for x in range(SIZE):
                c = image.getTexelColor(x, y)
                texels.append([c[0], c[1], c[2], c[3]])
        fields[shader_input.getName()] = texels
    baker.getImageHandler().clearImageCache()
    return {"material": path.stem, "width": SIZE, "height": SIZE, "fields": fields}


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    search = search_path()
    baker = mx_render_glsl.TextureBaker.create(SIZE, SIZE, mx_render.BaseType.FLOAT)
    for path in sorted(MATERIALS.glob("*.mtlx")):
        baked = bake(baker, path, search)
        (OUT / f"{path.stem}.json").write_text(json.dumps(baked, indent=1) + "\n")
        print("baked", path.stem, sorted(baked["fields"]))
    return 0


if __name__ == "__main__":
    sys.exit(main())
