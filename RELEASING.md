# Releasing

Every package carries the workspace version (`Cargo.toml`, `[workspace.package] version`); the Python wheels take it
from `crates/shadewire-py/Cargo.toml` and the Blender extension from `blender/shadewire/blender_manifest.toml`
(`version`, and the wheel file names in `wheels`). Bump all three together, update `CHANGELOG.md`, commit, then:

## crates.io

Nine crates, published together (Cargo orders them by their dependencies):

```
cargo publish --workspace --exclude shadewire-capi --exclude shadewire-capi-ctest --exclude shadewire-py --dry-run
cargo publish --workspace --exclude shadewire-capi --exclude shadewire-capi-ctest --exclude shadewire-py
```

`shadewire`, `shadewire-mtlx`, `shadewire-wgsl`, `shadewire-glsl`, `shadewire-hlsl`, `shadewire-msl`,
`shadewire-gdshader`, `shadewire-cli` and `shadewire-bevy`. The C API and the Python module are `publish = false`: they ship as binaries and on PyPI.
Publishing needs a crates.io token (`cargo login`). Rust 1.88 builds all but `shadewire-bevy` (1.97, Bevy's minimum).

## PyPI

Pushing a version tag runs `.github/workflows/wheels.yml`: six wheels (Windows x64 and ARM64, macOS x64 and arm64,
manylinux2014 x64 and aarch64; Python's stable ABI, CPython 3.8 and up) built and imported on their own runners, the
sdist, and the `pypi` job that uploads them with trusted publishing.

```
git tag v0.1.0
git push origin v0.1.0
```

Once, before the first tag: on pypi.org add a trusted publisher for the `shadewire` project (a "pending publisher"
before it exists): this repository, workflow `wheels.yml`, environment `pypi`; and create the `pypi` environment in
the repository's settings (optionally with required reviewers, so a tag waits for approval).

Without CI, the same files build locally (`python blender/build_extension.py ...` builds the wheels into
`<out>/wheels`, and `maturin sdist` in `crates/shadewire-py` the sdist), then
`twine check dist/*` and `twine upload dist/*`.

## Blender extensions (extensions.blender.org)

```
python blender/build_extension.py --blender <blender 4.3 or later> --out dist/blender
blender --command extension validate dist/blender/shadewire-0.1.0.zip
```

Then test the zip as users get it, on each platform, with a portable Blender (the oldest supported, 4.3, and the
latest) and a throwaway profile, as `blender/README.md` shows (`smoke_test.py --installed`).

One zip with the wheel of every platform Blender ships for (the Linux ARM64 wheel is PyPI's only). Upload it on
extensions.blender.org (Upload Extension), with `assets/logo/png/shadewire-mark-256.png` as the icon and
`assets/logo/png/shadewire-blender-featured-1920x1080.png` as the featured image. The preview image renders with the extension
installed (same throwaway profile): `blender -b -P blender/render_preview.py -- preview-1920x1080.png`. The add-on is GPL-3.0-or-later, as the platform requires
(`blender/shadewire/LICENSE`, the manifest's `license` and `copyright`).
