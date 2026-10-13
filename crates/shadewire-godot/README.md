# shadewire-godot

A Godot 4 extension (godot-rust 0.5.5, built against API 4.4 with `compatibility_minimum = "4.4"`; tested with
Godot 4.7.2) that turns MaterialX documents into `ShaderMaterial`s, compiled in-process by `shadewire-mtlx` and
`shadewire-gdshader`: at runtime and when the editor imports a `.mtlx` file.

This crate is outside the main workspace (gdext needs Rust 1.94): build it from this folder.

```bash
cargo build --release
```

The addon is `addons/shadewire`: copy it into a project and put the library next to the `.gdextension` file, as
`addons/shadewire/bin/shadewire_godot.dll`. Only the Windows x86_64 library is listed for now.

## Runtime

```gdscript
var sw := Shadewire.new()
var mat := sw.load_material("D:/assets/materials/rusty_metal.mtlx", {"material": "Rust"})
if mat == null:
    push_error(sw.get_last_error())
$Mesh.material_override = mat

# A document in a string; includes and textures resolve against the base (a directory or a document location).
var mat2 := sw.compile_string(text, "user://mods/metal/")
var names := sw.material_names("user://mods/metal/metal.mtlx")

# The resource loader: documents Godot did not import (user://, OS paths, res:// files kept as-is in an export).
var mat3: ShaderMaterial = load("user://mods/metal/metal.mtlx")
```

Sources are `res://`, `user://` and OS paths, read with `FileAccess`; `xi:include`s and texture files resolve
against the location of the document that names them. Textures from imported project files load as the imported
texture; anything else is decoded from its bytes: PNG, JPEG and WebP. A texture that cannot load leaves Godot's
default bound and warns, or fails the load with `strict_textures`. Materials with the same generated code share one
`Shader`. Each material carries its reflection as the `shadewire_reflection` metadata (params, uniforms, textures,
approximations).

Options (a dictionary for the runtime API; the importer's options in the Import dock):

| Option | Values | Default |
|---|---|---|
| `material` | a material name of the document | the first |
| `models` | `"mapped"` (named models on Godot's BRDF), `"lobes"` (everything lit by MaterialX's lobes), or an array of model names | `"mapped"` |
| `bake_constants` | authored values become constants instead of uniforms | `false` |
| `optimize` | run the optimizer | `true` |
| `transparency` | `auto`, `opaque`, `scissor`, `hash`, `blend` | `auto` |
| `emission_scale` | multiplies emission | `1.0` |
| `strict_textures` | a texture that cannot load fails the load | `false` |
| `write_gdshader` (importer only) | also write the shader next to the `.mtlx` | `false` |

## Editor

`.mtlx` files in the project import as `ShaderMaterial` resources (after their images, which Godot imports as
usual). Drag one onto a mesh's material slot. Shader parameters appear under the `material` group; re-importing
resets them to the document's values. A document with no material (an include) imports as an empty material. gdext
editor plugins are always on: the importer does not appear under Project Settings > Plugins.

Things to know:

- Godot imports PNGs without mipmaps by default and the shader samples with mipmaps, so such a texture aliases;
  the importer warns, and Mipmaps > Generate in the image's import options fixes it.
- The first time the editor opens a project with the addon, it finds the extension during its first file scan,
  after the file list is taken, so `.mtlx` files already there are not imported until they change (or the
  editor's file system cache in `.godot/editor` is removed).
- To load a `res://` `.mtlx` with the runtime API in an export, set the file's import mode to Keep File: exports
  contain imported resources, not the sources.

## Tests

`cargo test` runs the parts that do not need Godot. The smoke test needs a Godot binary and a GPU: it builds the
extension, assembles a project, imports it with the editor, then loads materials through the importer, the runtime
API, `compile_string` and the resource loader, draws them with the real renderer and saves a screenshot.

```bash
python -I tests/smoke/run.py <path to a Godot 4.4+ binary>
```

## Not done yet

From the plan in `docs/specs/2026-10-12-godot-analysis.md` (Sections 4.1, 4.3, 5, Phase 2):

- `http(s)://` sources: `load_material_async` with the `material_ready` / `material_failed` /
  `material_progress` signals, `HTTPClient` fetching on a worker thread, host-scoped auth headers and the callback,
  the disk cache (`cache_max_bytes`, LRU eviction, `evict_cache`, `clear_cache`). URLs are refused today, and a
  remote document already cannot reference local files.
- `list_dependencies` (prefetching) in `shadewire-mtlx`, and MaterialX `ui*` hints in reflection.
- Threaded import (`can_import_threaded` is false: loading the textures from an import worker thread hung the
  headless import in Godot 4.7.2), setting texture import options from the importer, the `Shadewire.cs` wrapper,
  Linux and macOS libraries, CI, runtime limits for untrusted documents.
