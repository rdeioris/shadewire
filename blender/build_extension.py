"""Builds the `shadewire` wheels for every platform Blender ships for, then one extension zip per platform.

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

HERE = os.path.dirname(os.path.abspath(__file__))
CRATE = os.path.join(HERE, "..", "crates", "shadewire-py")
ADDON = os.path.join(HERE, "shadewire_mtlx")

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
    parser.add_argument("--blender", required=True, help="the Blender executable (4.2 or later)")
    parser.add_argument("--out", default=os.path.join(HERE, "..", "target", "extension"))
    parser.add_argument("--skip-wheels", action="store_true", help="use the wheels already in shadewire_mtlx/wheels")
    args = parser.parse_args()

    wheels = os.path.join(ADDON, "wheels")
    if not args.skip_wheels:
        if os.path.isdir(wheels):
            shutil.rmtree(wheels)
        os.makedirs(wheels)
        build_wheels(wheels)
    print("wheels:", *sorted(os.path.basename(w) for w in glob.glob(os.path.join(wheels, "*.whl"))), sep="\n  ")
    os.makedirs(args.out, exist_ok=True)
    subprocess.run([args.blender, "--command", "extension", "build", "--split-platforms", "--source-dir", ADDON,
                    "--output-dir", args.out], check=True)


if __name__ == "__main__":
    main()
