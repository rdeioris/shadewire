"""Download the AMD GPUOpen MaterialX Library (https://matlib.gpuopen.com) for the regression harness.

For every published material: the "1k 8b" and "1k 16b" packages (extracted, the zip is not kept; about
200 materials have no 16-bit package) and the preview image of every render the site shows, the first
in the site's order being the reference (`preview.jpg`).

Run from the repo root (re-running skips what is already there):
    python -I tools/matlib/fetch.py [--data DIR] [--jobs 8] [--limit N] [--only NAME ...]

Layout under the data directory:
    index/{materials,packages,renders}.json   the API listings
    materials/<mtlx_material_name>/
        meta.json         title, ids, the .mtlx of each package, the reference render
        preview.jpg       the reference preview (the site's first render)
        renders/<id>.jpg  every render's preview
        1k_8b/  1k_16b/   the extracted packages
"""
import argparse
import concurrent.futures
import io
import json
import os
import pathlib
import re
import shutil
import sys
import time
import urllib.request
import zipfile

API = "https://api.matlib.gpuopen.com/api"
ROOT = pathlib.Path(__file__).resolve().parents[2]
# The data directory: --data, else $SHADEWIRE_MATLIB, else tools/matlib/_data (gitignored).
DEFAULT_DATA = pathlib.Path(os.environ.get("SHADEWIRE_MATLIB") or ROOT / "tools/matlib/_data")
PACKAGES = {"1k 8b": "1k_8b", "1k 16b": "1k_16b"}
USER_AGENT = "shadewire-matlib-harness/0.1"


def get(url, retries=4):
    """The body of `url` (redirects followed), retried with backoff."""
    for attempt in range(retries):
        try:
            request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
            with urllib.request.urlopen(request, timeout=120) as response:
                return response.read()
        except Exception as err:  # network errors and 5xx alike
            if attempt == retries - 1:
                raise RuntimeError(f"{url}: {err}") from err
            time.sleep(2**attempt)


def listing(endpoint):
    """Every result of a paginated API listing."""
    results = []
    url = f"{API}/{endpoint}/?limit=1000&offset=0"
    while url:
        page = json.loads(get(url))
        results.extend(page["results"])
        url = page["next"]
    return results


def load_index(data, refresh):
    index = data / "index"
    index.mkdir(parents=True, exist_ok=True)
    out = {}
    for name in ("materials", "packages", "renders"):
        path = index / f"{name}.json"
        if refresh or not path.is_file():
            path.write_text(json.dumps(listing(name), indent=1), encoding="utf-8")
        out[name] = json.loads(path.read_text(encoding="utf-8"))
    return out


def dir_name(material):
    return re.sub(r"[^A-Za-z0-9_.-]", "_", material["mtlx_material_name"] or material["id"])


def extract(blob, dest):
    """Extracts a package zip into `dest`, refusing entries that would land outside it."""
    tmp = dest.with_name(dest.name + ".partial")
    shutil.rmtree(tmp, ignore_errors=True)
    tmp.mkdir(parents=True)
    root = tmp.resolve()
    with zipfile.ZipFile(io.BytesIO(blob)) as archive:
        for info in archive.infolist():
            target = (tmp / info.filename).resolve()
            if not target.is_relative_to(root):
                raise RuntimeError(f"zip entry outside the package: {info.filename}")
            if info.is_dir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(archive.read(info))
    tmp.rename(dest)


def fetch_material(material, packages, renders, data):
    folder = data / "materials" / dir_name(material)
    folder.mkdir(parents=True, exist_ok=True)
    order = material["renders_order"] or material["renders"]
    meta = {
        "id": material["id"],
        "title": material["title"],
        "material_type": material["material_type"],
        "mtlx_material_name": material["mtlx_material_name"],
        "mtlx_filename": material["mtlx_filename"],
        "url": f"https://matlib.gpuopen.com/main/materials/all?material={material['id']}",
        "license": material["license"],
        "renders": order,
        "reference_render": order[0] if order else None,
        "packages": {},
    }
    for render_id in order:
        path = folder / "renders" / f"{render_id}.jpg"
        if not path.is_file():
            path.parent.mkdir(exist_ok=True)
            path.write_bytes(get(renders[render_id]["preview_url"]))
    if order:
        shutil.copyfile(folder / "renders" / f"{order[0]}.jpg", folder / "preview.jpg")
    for package_id in material["packages"]:
        package = packages.get(package_id)
        if not package or package["label"] not in PACKAGES:
            continue
        variant = PACKAGES[package["label"]]
        dest = folder / variant
        if not dest.is_dir():
            extract(get(package["file_url"]), dest)
        mtlx = sorted(dest.rglob("*.mtlx"))
        meta["packages"][variant] = {
            "id": package_id,
            "file": package["file"],
            "etag": package["etag"],
            "mtlx": [p.relative_to(folder).as_posix() for p in mtlx],
        }
    (folder / "meta.json").write_text(json.dumps(meta, indent=1), encoding="utf-8")
    return folder.name, sorted(meta["packages"])


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--data", type=pathlib.Path, default=DEFAULT_DATA)
    parser.add_argument("--jobs", type=int, default=8)
    parser.add_argument("--limit", type=int, help="only the first N materials")
    parser.add_argument("--only", nargs="*", help="only these mtlx material names")
    parser.add_argument("--refresh", action="store_true", help="download the API listings again")
    args = parser.parse_args()

    index = load_index(args.data, args.refresh)
    packages = {p["id"]: p for p in index["packages"]}
    renders = {r["id"]: r for r in index["renders"]}
    materials = [m for m in index["materials"] if m["status"] == "Published"]
    materials.sort(key=lambda m: m["mtlx_material_name"] or m["id"])
    if args.only:
        materials = [m for m in materials if m["mtlx_material_name"] in args.only]
    if args.limit:
        materials = materials[: args.limit]

    failures = 0
    with concurrent.futures.ThreadPoolExecutor(args.jobs) as pool:
        futures = {pool.submit(fetch_material, m, packages, renders, args.data): m for m in materials}
        for done, future in enumerate(concurrent.futures.as_completed(futures), 1):
            material = futures[future]
            try:
                name, variants = future.result()
                print(f"[{done}/{len(materials)}] {name}: {', '.join(variants)}", flush=True)
            except Exception as err:
                failures += 1
                print(f"[{done}/{len(materials)}] FAILED {material['mtlx_material_name']}: {err}", flush=True)
    print(f"{len(materials) - failures} materials fetched, {failures} failed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
