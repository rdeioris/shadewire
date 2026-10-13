"""Smoke-test the Shadewire extension in Godot with the real renderer.

Run from anywhere, with a Godot 4.4+ binary (the console build on Windows) and a GPU:
    python -I crates/shadewire-godot/tests/smoke/run.py <path to godot> [screenshot.png]

It builds the extension, assembles a project in crates/shadewire-godot/target/smoke (this folder, the addon with
the library, and the vendored grid.png), imports it with the editor (the importer runs), then runs smoke.gd, which
loads materials through the importer, the runtime API and the resource loader, draws them and saves a screenshot
(default target/smoke/smoke.png). Exits 1 on a failed check or any shader error.
"""
import pathlib
import shutil
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
CRATE = HERE.parent.parent
ROOT = CRATE.parent.parent
EXAMPLES = ROOT / "vendor" / "materialx" / "resources" / "Materials" / "Examples"
VENDORED = EXAMPLES / "StandardSurface" / "standard_surface_marble_solid.mtlx"


def godot(binary, project, *args):
    run = subprocess.run(
        [binary, "--path", str(project), *args],
        capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=600,
    )
    return run.returncode, run.stdout + run.stderr


def main():
    if len(sys.argv) not in (2, 3):
        sys.exit(__doc__)
    binary = sys.argv[1]
    work = CRATE / "target" / "smoke"
    screenshot = pathlib.Path(sys.argv[2]).resolve() if len(sys.argv) == 3 else work / "smoke.png"
    subprocess.run(["cargo", "build", "-q"], cwd=CRATE, check=True)

    shutil.rmtree(work, ignore_errors=True)
    shutil.copytree(HERE, work, ignore=shutil.ignore_patterns("run.py", "__pycache__"))
    shutil.copytree(CRATE / "addons", work / "addons", ignore=shutil.ignore_patterns(".gitignore"))
    (work / "addons" / "shadewire" / "bin").mkdir(exist_ok=True)
    shutil.copy(CRATE / "target" / "debug" / "shadewire_godot.dll", work / "addons" / "shadewire" / "bin")
    (work / "smoke" / "textures").mkdir()
    shutil.copy(ROOT / "vendor" / "materialx" / "resources" / "Images" / "grid.png", work / "smoke" / "textures")

    # Registered up front: an extension the editor first meets during its first scan loads after the file list
    # is taken, and the .mtlx files it lists stay unimported.
    (work / ".godot").mkdir()
    (work / ".godot" / "extension_list.cfg").write_text("res://addons/shadewire/shadewire.gdextension\n")
    code, log = godot(binary, work, "--headless", "--import")
    imported = list((work / ".godot" / "imported").glob("textured.mtlx-*.res"))
    if code != 0 or not imported or "ERROR:" in log:
        print(log)
        sys.exit(f"import failed (exit {code}, {len(imported)} imported .mtlx)")
    for line in log.splitlines():
        if "shadewire:" in line:
            print("import:", line)

    args = ["--resolution", "960x300", "--script", "res://smoke.gd", "--", VENDORED.as_posix(), str(screenshot)]
    code, log = godot(binary, work, *args)
    print(log)
    errors = [line for line in log.splitlines() if "SHADER ERROR" in line or "ERROR:" in line]
    if code != 0 or errors or "FAILURES 0" not in log:
        sys.exit(f"smoke test failed (exit {code}, {len(errors)} error lines)")
    print(f"smoke test passed; screenshot {screenshot}")


if __name__ == "__main__":
    main()
