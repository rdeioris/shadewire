"""Builds the Shadewire C library and stages it into the Unreal plugin's ThirdParty module.

    python unreal/stage_library.py [--no-build]

Copies crates/shadewire-capi/include/shadewire.h and this platform's shared library (with the import library on
Windows) from target/release into unreal/Shadewire/Source/ThirdParty/ShadewireLibrary/.
"""

import argparse
import pathlib
import platform
import shutil
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
MODULE = ROOT / "unreal" / "Shadewire" / "Source" / "ThirdParty" / "ShadewireLibrary"

# Unreal's platform folder and the files cargo writes for a cdylib, per host system.
PLATFORMS = {
    "Windows": ("Win64", ["shadewire_capi.dll", "shadewire_capi.dll.lib"]),
    "Darwin": ("Mac", ["libshadewire_capi.dylib"]),
    "Linux": ("Linux", ["libshadewire_capi.so"]),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--no-build", action="store_true", help="stage what target/release already holds")
    args = parser.parse_args()

    folder, files = PLATFORMS[platform.system()]
    if not args.no_build:
        subprocess.run(["cargo", "build", "--release", "-p", "shadewire-capi"], cwd=ROOT, check=True)

    include = MODULE / "include"
    include.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ROOT / "crates" / "shadewire-capi" / "include" / "shadewire.h", include / "shadewire.h")
    target = MODULE / folder
    target.mkdir(parents=True, exist_ok=True)
    for name in files:
        shutil.copy2(ROOT / "target" / "release" / name, target / name)
        print(f"staged {target / name}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
