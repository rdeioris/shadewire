"""Builds the six `shadewire` wheels (the PyPI set, into <out>/wheels), then the extension zip: one file for every
platform Blender ships for, holding the manifest's `wheels` (copied into shadewire/wheels); Blender installs the one
for its platform. Blender has no Linux ARM64 build, so that wheel is for PyPI only.

python blender/build_extension.py --blender <blender executable> [--out DIR] [--skip-wheels]

Needs maturin and, for the Linux and macOS cross builds, zig (`pip install maturin ziglang`), plus the Rust
targets (`rustup target add aarch64-pc-windows-msvc aarch64-unknown-linux-gnu x86_64-apple-darwin
aarch64-apple-darwin x86_64-unknown-linux-gnu`). The wheels use Python's stable ABI from 3.8, so each serves
every Blender. CI builds the same wheels on native runners (`.github/workflows/wheels.yml`).
"""

import argparse
import glob
import os
import shutil
import subprocess
import sys
import tomllib

HERE = os.path.dirname(os.path.abspath(__file__))
CRATE = os.path.join(HERE, "..", "crates", "shadewire-py")
ADDON = os.path.join(HERE, "shadewire")

# (Rust target, extra maturin arguments): Windows natively, the rest through zig.
TARGETS = [
    ("x86_64-pc-windows-msvc", []),
    ("aarch64-pc-windows-msvc", []),
    ("x86_64-unknown-linux-gnu", ["--zig", "--compatibility", "manylinux2014"]),
    ("aarch64-unknown-linux-gnu", ["--zig", "--compatibility", "manylinux2014"]),
    ("x86_64-apple-darwin", ["--zig"]),
    ("aarch64-apple-darwin", ["--zig"]),
]


def zig_on_path():
    """The `ziglang` package keeps zig in its own folder: put it on PATH for maturin."""
    try:
        import ziglang
    except ImportError:
        return
    os.environ["PATH"] = os.path.dirname(ziglang.__file__) + os.pathsep + os.environ["PATH"]


def build_wheels(out):
    zig_on_path()
    for target, extra in TARGETS:
        print(f"== {target}", flush=True)
        subprocess.run([sys.executable, "-m", "maturin", "build", "--release", "--target", target, "-o", out, *extra],
                       cwd=CRATE, check=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--blender", required=True, help="the Blender executable (4.3 or later)")
    parser.add_argument("--out", default=os.path.join(HERE, "..", "target", "extension"))
    parser.add_argument("--skip-wheels", action="store_true", help="use the wheels already in <out>/wheels")
    args = parser.parse_args()

    built = os.path.join(args.out, "wheels")
    if not args.skip_wheels:
        if os.path.isdir(built):
            shutil.rmtree(built)
        os.makedirs(built)
        build_wheels(built)
    print("wheels:", *sorted(os.path.basename(w) for w in glob.glob(os.path.join(built, "*.whl"))), sep="\n  ")

    # The add-on gets exactly the wheels its manifest lists.
    with open(os.path.join(ADDON, "blender_manifest.toml"), "rb") as f:
        listed = [os.path.basename(w) for w in tomllib.load(f)["wheels"]]
    wheels = os.path.join(ADDON, "wheels")
    if os.path.isdir(wheels):
        shutil.rmtree(wheels)
    os.makedirs(wheels)
    for name in listed:
        shutil.copy(os.path.join(built, name), wheels)

    subprocess.run([args.blender, "--command", "extension", "build", "--source-dir", ADDON, "--output-dir", args.out],
                   check=True)


if __name__ == "__main__":
    main()
