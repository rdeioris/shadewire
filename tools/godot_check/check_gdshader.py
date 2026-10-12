"""Check every shader the Godot back end generates with Godot's own shader compiler.

Run from the repo root, with a Godot 4.x binary (the console build on Windows):
    python -I tools/godot_check/check_gdshader.py <path to godot>

It writes the shaders of shadewire-gdshader's tests (the vendored MaterialX examples, with values
as uniforms and baked in, and the emitter's own test shaders) to tools/godot_check/shaders, then
compiles each one in `godot --headless`. No GPU is needed: the dummy renderer runs the full
compiler. Exits 1 on any shader error or warning.
"""
import os
import pathlib
import shutil
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    godot = sys.argv[1]
    shaders = HERE / "shaders"
    shutil.rmtree(shaders, ignore_errors=True)
    env = dict(os.environ, SHADEWIRE_GDSHADER_OUT=str(shaders))
    subprocess.run(["cargo", "test", "-q", "-p", "shadewire-gdshader"], env=env, check=True)
    expected = len(list(shaders.glob("*.gdshader")))
    run = subprocess.run(
        [godot, "--headless", "--path", str(HERE), "--script", "check.gd"],
        capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=600,
    )
    log = run.stdout + run.stderr
    problems = [line for line in log.splitlines() if "ERROR" in line or "WARNING" in line]
    if f"CHECKED {expected}" not in log or problems:
        print(log)
        sys.exit(f"{len(problems)} error or warning lines; expected {expected} shaders checked")
    print(f"{expected} shaders compiled in Godot without errors or warnings")


if __name__ == "__main__":
    main()
