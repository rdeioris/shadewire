# Shadewire for Unreal Engine

An Unreal Engine editor plugin (`Shadewire/`) that exports materials and material instances as MaterialX documents
and MDL modules, with their textures. The plugin dumps each material's graph as it is in the editor; Shadewire's
library (`shadewire_capi`) lowers it and writes the file. Built and tested with Unreal Engine 5.8 on Windows.

Right-click materials or material instances in the Content Browser: **Shadewire > Export MaterialX...**,
**Export MDL...** or **Save Shadewire Graph...** (the JSON document `shadewire-unreal` reads). Each material becomes
one file named after it in the folder you pick, its textures under `textures/` beside it. What was dropped or
approximated is listed in the Message Log under Shadewire.

From the command line:

```bash
UnrealEditor-Cmd MyProject.uproject -run=ShadewireExport -folder=/Game/Materials -out=C:/Export -format=mtlx
```

`-materials=/Game/A.A,/Game/B.B` names materials instead of a folder; `-format` is `json`, `mtlx` or `mdl`; `-lobes`
writes MaterialX as a closure tree instead of `open_pbr_surface`; `-notextures` skips the texture files.

## USD export

With the plugin enabled, Unreal's own USD level export (File > Export All / Export Selected to `.usd`, or a Python
`AssetExportTask` with `LevelExporterUSDOptions`) writes materials as Shadewire MaterialX when **Bake Materials** is
on: each material's layer (`Assets/<Material>.usda`) is a `Material` whose prim references
`<Material>.mtlx` (`</MaterialX/Materials/<Material>>`) beside it, with the textures in the export's texture folder,
and keeps Unreal's own `unreal` surface output for re-import. Unreal binds those layers to the meshes as it binds its
baked ones, so skeletal meshes keep their UsdSkel rig and every section gets its MaterialX material. A material the
library fails on is baked as Unreal would, with a warning. Editor Preferences > Plugins > Shadewire turns this off
or writes closure trees instead of `open_pbr_surface`.

How: the level export exports each material with an export task that names no exporter, and the engine picks the
exporter registered for the material's most derived class. The plugin registers exporters for `UMaterial` and
`UMaterialInstance`, below Unreal's (registered for `UMaterialInterface`). Exporting a single static or skeletal mesh
asset from the Content Browser bakes its materials with Unreal's exporter directly, so it is not covered; export the
mesh in a level instead.

## Building

The plugin loads Shadewire's C library from its own `Binaries/<platform>` folder, where the build copies it (a
packaged plugin, `RunUAT BuildPlugin`, carries it there too). Build and stage it first, from the repository:

```bash
python unreal/stage_library.py
```

then build the plugin with the project that uses it (or `RunUAT BuildPlugin`). The test project here uses the plugin
from this folder:

```bash
D:/Epic/UE_5.8/Engine/Build/BatchFiles/Build.bat UnrealEditor Win64 Development -Project=unreal/ShadewireTest/ShadewireTest.uproject
```

## Fixtures

`make_fixtures.py` builds the test materials in `ShadewireTest` (under `/Game/Fixtures`) with the editor's Python;
the commandlet dumps them to `fixtures/graphs/`, which `shadewire-unreal`'s tests read:

```bash
UnrealEditor-Cmd unreal/ShadewireTest/ShadewireTest.uproject -run=pythonscript -script=<repo>/unreal/make_fixtures.py
UnrealEditor-Cmd unreal/ShadewireTest/ShadewireTest.uproject -run=ShadewireExport -folder=/Game/Fixtures -out=<repo>/unreal/fixtures/graphs
```

## What is exported

Legacy shading models (Default Lit, Unlit, Clear Coat, Subsurface, Cloth, Two Sided Foliage, Thin Translucent) with
the opaque, masked and translucent blend modes, and Substrate materials (slabs, simple clear coat, unlit, and the
mixing, layering, add, weight and select operators) as MaterialX closure trees and MDL `::df` trees; parameters as
editable inputs, instance overrides applied; material functions inlined, engine functions included. Custom HLSL and
the expressions that only mean something inside Unreal's renderer (scene textures, depths) become a warning and a
neutral value. Substrate translucency is not exported (the material is opaque). See
`crates/shadewire-unreal/README.md` for the mapping.

The Substrate fixtures need Substrate on, which `make_substrate_fixtures.py` and the commandlet get from the command
line rather than a project setting:

```bash
UnrealEditor-Cmd unreal/ShadewireTest/ShadewireTest.uproject -run=pythonscript -script=<repo>/unreal/make_substrate_fixtures.py "-ini:Engine:[/Script/Engine.RendererSettings]:r.Substrate=True"
```
