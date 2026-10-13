"""Check every shader the Godot back end generates with Godot's own shader compiler and renderer.

Run from the repo root, with a Godot 4.x binary (the console build on Windows) and a GPU:
    python -I tools/godot_check/check_gdshader.py <path to godot>

It writes to tools/godot_check/shaders the shaders of shadewire-gdshader's tests and, through the CLI, every
material of the vendored MaterialX examples four ways (mapped onto Godot's BRDF or lit by MaterialX's lobes, values
as uniforms or baked in). Then Godot draws each one for a frame with its default renderer. The headless (dummy)
renderer is not used: it accepts code the real renderers refuse. Exits 1 on any shader error or warning.
"""
import os
import pathlib
import shutil
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
EXAMPLES = ROOT / "vendor" / "materialx" / "resources" / "Materials" / "Examples"
# Materials the front end refuses for every back end (no implementation of chiang_hair_roughness).
SKIP = {"simple_hair_default.mtlx"}


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    godot = sys.argv[1]
    shaders = HERE / "shaders"
    shutil.rmtree(shaders, ignore_errors=True)
    shaders.mkdir()
    env = dict(os.environ, SHADEWIRE_GDSHADER_OUT=str(shaders))
    subprocess.run(["cargo", "test", "-q", "-p", "shadewire-gdshader"], env=env, cwd=ROOT, check=True)
    subprocess.run(["cargo", "build", "-q", "-p", "shadewire-cli"], cwd=ROOT, check=True)
    target = ROOT / os.environ.get("CARGO_TARGET_DIR", "target")
    cli = str(target / "debug" / "shadewire")
    for path in sorted(EXAMPLES.glob("*/*.mtlx")):
        if path.name in SKIP:
            continue
        names = subprocess.run([cli, "list", str(path)], capture_output=True, text=True, check=True).stdout.split()
        for material in names:
            for mode in ([], ["--lobes"]):
                for values in ([], ["--constants"]):
                    stem = "_".join([path.stem, material] + [a.lstrip("-") for a in mode + values])
                    out = shaders / f"{stem}.gdshader"
                    args = [cli, "compile", str(path), "--material", material, "--lang", "gdshader", "-o", str(out)]
                    subprocess.run(args + mode + values, check=True)
    expected = len(list(shaders.glob("*.gdshader")))
    run = subprocess.run(
        [godot, "--path", str(HERE), "--script", "check.gd"],
        capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=1200,
    )
    log = run.stdout + run.stderr
    problems = [line for line in log.splitlines() if "ERROR" in line or "WARNING" in line]
    if f"CHECKED {expected}" not in log or problems:
        print(log)
        sys.exit(f"{len(problems)} error or warning lines; expected {expected} shaders checked")
    print(f"{expected} shaders compiled and drawn in Godot without errors or warnings")


if __name__ == "__main__":
    main()
