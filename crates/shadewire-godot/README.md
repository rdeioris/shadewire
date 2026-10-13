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

Sources are `res://`, `user://` and OS paths, read with `FileAccess`, and with `load_material_async` also
`http(s)://` URLs (below); `xi:include`s and texture files resolve against the location of the document that names
them. Textures from imported project files load as the imported
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
| `max_nodes` (runtime only) | IR nodes the material may elaborate to; over it the load fails | `4096` |
| `max_textures` (runtime only) | textures the shader may sample; over it the load fails | `16` |
| `max_loop_iterations` (runtime only) | iterations of each loop whose count the document controls (fractal noise octaves, also when they are a uniform), clamped in the shader | `16` |

The `max_*` options are limits for untrusted documents (mods, downloads), on for every runtime load:
`load_material`, `load_material_async`, `compile_string` and the resource loader (which always uses the defaults).
`0` turns one off. Imports in the editor are the project's own files and have none. The largest vendored and
matlib materials need about 550 nodes and 7 textures. The node limit is checked while the graph is built, so
nodegraphs instanced inside one another cannot grow it exponentially first. Loops with a fixed count (Worley
noise's neighbours, thin-film terms) are not clamped. Error messages never quote an included file: an include that
is not a well-formed `<materialx>` document fails with its location only.

Network options, for `load_material_async` only (`load_material` refuses them):

| Option | Values | Default |
|---|---|---|
| `allow_http` | allow plain `http://` (HTTPS only otherwise) | `false` |
| `headers` | `PackedStringArray` of `"Name: value"`, sent to the document's own host | none |
| `headers_all_hosts` | send `headers` to every host the document names | `false` |
| `cache_policy` | `use` (serve cached copies as they are), `revalidate`, `bypass` (do not read the cache; the fresh response replaces it), `offline` (cache only) | `revalidate` |
| `cache_authenticated` | cache responses fetched with headers | `true` |
| `connect_timeout` | seconds to resolve and connect | `10` |
| `timeout` | seconds per request, redirects included | `60` |
| `max_redirects` | redirects followed per request | `5` |
| `max_response_bytes` | bytes per response | 64 MiB |
| `max_material_bytes` | bytes per material: document, includes and textures | 256 MiB |

## Remote sources

```gdscript
var sw := Shadewire.new()  # keep it alive until the request ends
sw.material_ready.connect(func(id, mat): $Mesh.material_override = mat)
sw.material_failed.connect(func(id, message): push_error(message))
sw.material_progress.connect(func(id, done_bytes, total_bytes): pass)
var id := sw.load_material_async("https://cdn.example.com/materials/rusty_metal.mtlx", {"material": "Rust"})
sw.cancel(id)  # optional: ends with material_failed(id, "cancelled")

# Headers scoped to hosts: exact, *.domain (subdomains only), optionally :port. An empty array removes a pattern.
sw.set_http_headers("cdn.example.com", PackedStringArray(["Authorization: Bearer " + token]))
sw.set_http_headers("*.assets.example.org", PackedStringArray(["X-Api-Key: " + key]))
sw.http_headers_callback = func(url: String) -> PackedStringArray: return my_signer.sign(url)

sw.cache_max_bytes = 512 * 1024 * 1024  # default 256 MiB; 0 turns the cache off
sw.cache_dir = "user://shadewire_cache"  # default
sw.get_cache_size()
sw.evict_cache(url)
sw.clear_cache()
```

`load_material_async` takes a URL or a local path. A worker thread fetches the document, its includes (as the
front end meets them) and then all remote textures in parallel, and compiles; the main thread creates the
`Shader`, decodes the textures and emits the signal. Events reach the main thread through the scene tree's
`process_frame`, so the main loop must be a `SceneTree`. `load_material` stays local: given a URL it fails, so the
main thread never waits on the network.

- **Fetching**: Godot's `HTTPClient` on the worker thread, polled every millisecond (it follows the project's TLS
  certificate bundle; no proxy support yet). One connection per request.
- **Where references go**: includes and textures resolve against the document's URL (RFC 3986: `t.png`, `../t.png`,
  `/t.png`, `//host/t.png`), and may name any host. A remote document cannot reference local files.
- **Headers**: every `set_http_headers` pattern matching the request's host applies; on a name clash the most specific
  wins (exact over wildcard, longer over shorter, with a port over without). The callback runs on the main thread
  for each request (the worker waits for it) and its headers come last. Per-request `headers` go to the document's
  host only, unless `headers_all_hosts`. Each redirect hop is matched again: a redirect to another host gets that
  host's headers, never the original host's, nor the per-request ones. Header values never appear in errors, logs
  or cache keys.
- **Cache**: one file per URL plus `index.json` under `cache_dir`, keyed by the URL the document names (without the
  fragment, never with headers), with its `ETag` / `Last-Modified`. `revalidate` asks with `If-None-Match` /
  `If-Modified-Since` and serves the copy on `304`. Responses with `Cache-Control: no-store` are not written. Least
  recently used entries go after each write and when the cap is lowered; entries an in-flight request uses wait for
  it. The cache belongs to its directory: `Shadewire` objects using the same directory share it and its cap.

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

`cargo test` runs the parts that do not need Godot: locations, URLs, host patterns and header merging, the cache
index and LRU, the fetch rules (redirects, header scoping, caps, cache policies) over a fake transport, and the
limits for untrusted documents. The
smoke test needs a Godot binary and a GPU: it builds the extension, assembles a project, imports it with the editor,
then loads materials through the importer, the runtime API, `compile_string` and the resource loader, draws them
with the real renderer and saves a screenshot. For `load_material_async` it serves a document from two local HTTP
servers (`127.0.0.1` and `localhost`, plain http through `allow_http`) and checks that the document's relative
include and textures load, that a texture on the second host and one redirected there load, that the second load is
revalidated from the cache (`304`s) and a `use` load makes no request, that the auth header scoped to the first host
never reaches the second, and that lowering `cache_max_bytes` evicts down to the cap.

```bash
python -I tests/smoke/run.py <path to a Godot 4.4+ binary>
```

## Not done yet

From the plan in `docs/specs/2026-10-12-godot-analysis.md` (Sections 4.1, 4.3, 5, Phase 2):

- Network: proxies, keep-alive connection reuse, includes fetched in parallel (they are fetched one by one as
  the front end reads them; textures are parallel).
- `list_dependencies` (prefetching) in `shadewire-mtlx`, and MaterialX `ui*` hints in reflection.
- Threaded import (`can_import_threaded` is false: loading the textures from an import worker thread hung the
  headless import in Godot 4.7.2), setting texture import options from the importer, the `Shadewire.cs` wrapper,
  Linux and macOS libraries, CI.
- Limits: a byte cap on local documents (only remote ones have `max_material_bytes`), and a texture-size cap.
