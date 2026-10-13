"""Regression harness over the AMD GPUOpen MaterialX Library (fetched by fetch.py).

Every package (1k 8-bit and 1k 16-bit) is rendered on a sphere framed and UV-mapped like the library's
preview, by one of two renderers (--renderer):
  - pathtracer: the shadewire-pathtracer `shadewire-pt batch` command (the default), lit by the studio
    environment of studio_env.py (written to <data>/env/studio.hdr when missing) with the exposure and
    tonemapping of PATHTRACER_ARGS; its path is $SHADEWIRE_PT, else target/release/shadewire-pt.exe;
  - bevy: the shadewire-bevy `matlib_render` example (a rasterizer: one light, no reflections).
Each shot is then compared
  - with the library's own preview render (path traced, so never equal: the score tracks how close we
    get, and a score that drops is a regression), and
  - with the accepted baseline run (our own earlier shots: any visible change is flagged).
A package that used to build and render and no longer does is a regression too.

Run from the repo root (the data directory is --data, else $SHADEWIRE_MATLIB, else tools/matlib/_data):
    cargo build -p shadewire-pathtracer --release          # or: -p shadewire-bevy --example matlib_render
    python -I tools/matlib/harness.py calibrate [--only NAME ...]   # once: each material's sphere UV scale
    python -I tools/matlib/harness.py run [--only NAME ...] [--variants 1k_8b 1k_16b]
    python -I tools/matlib/harness.py compare <run dir>     # again, e.g. after accepting
    python -I tools/matlib/harness.py accept <run dir>      # make a run the baseline

`run` and `calibrate` take --renderer and --renderer-args (the renderer's options as one string, which
replace its defaults, e.g. "--spp 512 --env other.hdr"); a run records both. `run` renders into <data>/runs/<timestamp>/
(shots/, results.json), compares, writes report.html and report.json there, and exits 1 when anything
regressed against the baseline of its renderer (<data>/baselines/<renderer>/).

The previews tile each material at a scale of their own, not the one in its .mtlx; `calibrate` renders
every material at the scales of SCALES, keeps in <data>/calibration.json the one whose pattern best
matches the preview (the shading never matches: see `detail`), and `run` puts each material on a sphere
with its UVs multiplied by that scale. The material graph is untouched.
"""
import argparse
import datetime
import html
import json
import os
import pathlib
import shutil
import subprocess
import sys
import urllib.parse

import numpy as np
from PIL import Image

# `python -I` leaves the script's directory off the path; studio_env lives next to this file.
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import studio_env  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parents[2]
DEFAULT_DATA = pathlib.Path(os.environ.get("SHADEWIRE_MATLIB") or ROOT / "tools/matlib/_data")
EXE_SUFFIX = ".exe" if os.name == "nt" else ""

# name: (executable, arguments before the list and results files, how to build it). Both read the same list
# (`<mtlx>\t<out.png>\t<uv scale>`) and append the same start/ok/error results.
RENDERERS = {
    "pathtracer": (
        pathlib.Path(os.environ.get("SHADEWIRE_PT") or ROOT / f"target/release/shadewire-pt{EXE_SUFFIX}"),
        ["batch"],
        "cargo build -p shadewire-pathtracer --release (or set SHADEWIRE_PT to the executable)",
    ),
    "bevy": (
        ROOT / f"target/release/examples/matlib_render{EXE_SUFFIX}",
        [],
        "cargo build -p shadewire-bevy --example matlib_render --release",
    ),
}
DEFAULT_RENDERER = "pathtracer"

# The path tracer's default options, `{env}` being the studio map. Tuned on 30 random materials against
# the previews: median luminance inside the sphere 1.03x theirs (-1 stop gave 0.82x, 0 stops 1.14x).
PATHTRACER_ARGS = "--spp 128 --env {env} --tonemap aces --exposure=-0.5"


def renderer_args(data, renderer, given):
    """The renderer's options: `given` (one string) when not empty, else its defaults."""
    if given:
        return given.split()
    if renderer != "pathtracer":
        return []
    env = data / "env" / "studio.hdr"
    if not env.is_file():
        env.parent.mkdir(parents=True, exist_ok=True)
        studio_env.write_hdr(env, studio_env.environment(1024, studio_env.KEY, studio_env.AMBIENT))
    return PATHTRACER_ARGS.format(env=env.as_posix()).split()
VARIANTS = ("1k_8b", "1k_16b")

# Images are compared at this size, inside a centered disk of this radius (fraction of the size): the
# sphere covers it in both renders, so the backdrop and the silhouette's edge stay out of the scores.
SIZE = 256
DISK = 0.45

# Per-entry seconds the renderer may take before it counts as hung.
SECONDS_PER_ENTRY = 60

# Regression thresholds: RMSE (sRGB, 0..1) against the baseline shot, and loss of reference score.
BASELINE_RMSE = 0.01
REFERENCE_LOSS = 0.01


# ---------------------------------------------------------------------------------------------- render


def load_calibration(data):
    """{material folder: {"uv_scale": ..., "scores": {...}}} from calibrate, or {}."""
    path = data / "calibration.json"
    return json.loads(path.read_text(encoding="utf-8")) if path.is_file() else {}


def entries(data, only, variants):
    """(key, mtlx, shot name, UV scale) of every package to render, sorted by key."""
    calibration = load_calibration(data)
    out = []
    for meta_path in sorted((data / "materials").glob("*/meta.json")):
        folder = meta_path.parent
        if only and folder.name not in only:
            continue
        meta = json.loads(meta_path.read_text(encoding="utf-8"))
        for variant in variants:
            package = meta["packages"].get(variant)
            if not package:
                continue
            files = package["mtlx"]
            for mtlx in files:
                stem = pathlib.PurePosixPath(mtlx).stem
                key = f"{folder.name}/{variant}" + (f"/{stem}" if len(files) > 1 else "")
                uv_scale = calibration.get(folder.name, {}).get("uv_scale", 1.0)
                out.append((key, folder / mtlx, key.replace("/", "__") + ".png", uv_scale))
    return out


def read_results(path):
    """{shot path: (status, message)} and the shot started last without an outcome, from results.tsv."""
    done, pending = {}, None
    if path.is_file():
        for line in path.read_text(encoding="utf-8").splitlines():
            parts = line.split("\t")
            if parts[0] == "start":
                pending = parts[1]
            elif parts[0] in ("ok", "error"):
                done[parts[1]] = (parts[0], parts[2] if len(parts) > 2 else "")
                pending = None
    return done, pending


def render(run, todo, renderer, renderer_args):
    """Renders `todo` into run/shots with `renderer`, resuming after crashes and hangs; {key: result}."""
    exe, leading, build = RENDERERS[renderer]
    if not exe.is_file():
        sys.exit(f"error: no {exe}; build it with: {build}")
    shots = run / "shots"
    shots.mkdir(parents=True, exist_ok=True)
    tsv = run / "results.tsv"
    by_shot = {(shots / name).as_posix(): key for key, _, name, _ in todo}
    crashed = {}
    while True:
        done, _ = read_results(tsv)
        remaining = [e for e in todo if (shots / e[2]).as_posix() not in done and e[0] not in crashed]
        if not remaining:
            break
        listing = run / "list.tsv"
        lines = (f"{m.as_posix()}\t{(shots / n).as_posix()}\t{scale}\n" for _, m, n, scale in remaining)
        listing.write_text("".join(lines), "utf-8")
        print(f"rendering {len(remaining)} packages", flush=True)
        log = (run / "render.log").open("a", encoding="utf-8")
        try:
            code = subprocess.run(
                [str(exe), *leading, listing.as_posix(), tsv.as_posix(), *renderer_args],
                stdout=log,
                stderr=subprocess.STDOUT,
                timeout=SECONDS_PER_ENTRY * len(remaining) + 60,
            ).returncode
            reason = f"renderer exited with {code}"
        except subprocess.TimeoutExpired:
            code, reason = None, "renderer timed out"
        finally:
            log.close()
        done, pending = read_results(tsv)
        if pending and pending not in done:
            key = by_shot.get(pending, pending)
            crashed[key] = reason
            print(f"  crash on {key}: {reason}", flush=True)
        elif code != 0:
            # Died between entries: nothing to blame, so stop instead of looping.
            print(f"  {reason} with no entry in progress; stopping", flush=True)
            break
    done, _ = read_results(tsv)
    results = {}
    for key, mtlx, name, uv_scale in todo:
        shot = (shots / name).as_posix()
        if shot in done:
            status, message = done[shot]
        elif key in crashed:
            status, message = "crash", crashed[key]
        else:
            status, message = "missing", "not rendered"
        results[key] = {
            "status": status,
            "message": message,
            "mtlx": mtlx.as_posix(),
            "shot": f"shots/{name}",
            "uv_scale": uv_scale,
        }
    return results


# --------------------------------------------------------------------------------------------- compare


def load(path):
    """An image as float sRGB in [0, 1], SIZE x SIZE x 3."""
    with Image.open(path) as image:
        return np.asarray(image.convert("RGB").resize((SIZE, SIZE), Image.LANCZOS), dtype=np.float64) / 255.0


def disk():
    """The comparison disk as a boolean SIZE x SIZE mask."""
    y, x = np.mgrid[0:SIZE, 0:SIZE]
    c = (SIZE - 1) / 2
    return (x - c) ** 2 + (y - c) ** 2 <= (DISK * SIZE) ** 2


MASK = disk()


def box(image, radius=3):
    """Mean over a (2r+1)^2 window (edges clamped), via summed-area tables."""
    padded = np.pad(image, radius + 1, mode="edge")
    sat = padded.cumsum(0).cumsum(1)
    n = 2 * radius + 1
    window = sat[n:, n:] - sat[:-n, n:] - sat[n:, :-n] + sat[:-n, :-n]
    return window[: image.shape[0], : image.shape[1]] / (n * n)


def ssim(a, b):
    """Mean SSIM of the luminance of two images inside the disk."""
    weights = np.array([0.2126, 0.7152, 0.0722])
    la, lb = a @ weights, b @ weights
    c1, c2 = 0.01**2, 0.03**2
    ma, mb = box(la), box(lb)
    va, vb = box(la * la) - ma * ma, box(lb * lb) - mb * mb
    cov = box(la * lb) - ma * mb
    s = ((2 * ma * mb + c1) * (2 * cov + c2)) / ((ma * ma + mb * mb + c1) * (va + vb + c2))
    return float(s[MASK].mean())


def scores(a, b):
    """RMSE, mean-color distance (both sRGB, 0..1) and luminance SSIM of two images inside the disk."""
    diff = a[MASK] - b[MASK]
    return {
        "rmse": float(np.sqrt((diff**2).mean())),
        "mean_color": float(np.linalg.norm(a[MASK].mean(0) - b[MASK].mean(0))),
        "ssim": ssim(a, b),
    }



def baseline_of(data, renderer):
    return data / "baselines" / renderer


def compare(data, run):
    """Scores a run against the library previews and its renderer's baseline."""
    settings = json.loads((run / "run.json").read_text(encoding="utf-8"))
    # A run that left packages out does not report the baseline's others as missing.
    subset = bool(settings["only"]) or set(settings["variants"]) != set(VARIANTS)
    results = json.loads((run / "results.json").read_text(encoding="utf-8"))
    baseline_dir = baseline_of(data, settings.get("renderer", "bevy"))
    baseline_path = baseline_dir / "results.json"
    baseline = json.loads(baseline_path.read_text(encoding="utf-8")) if baseline_path.is_file() else {}
    report = {}
    for key, result in results.items():
        name = key.split("/")[0]
        row = {"status": result["status"], "message": result["message"], "regressions": []}
        preview = data / "materials" / name / "preview.jpg"
        shot = run / result["shot"]
        old = baseline.get(key)
        if result["status"] == "ok" and shot.is_file():
            ours = load(shot)
            if preview.is_file():
                reference = load(preview)
                row["reference"] = scores(ours, reference)
                # Pixels never line up with the previews (another renderer, its own lighting): the score
                # is how well the preview's pattern is found in the shot, wherever it lies.
                row["reference"]["score"] = pattern_match(ours, reference)
            if old and old.get("status") == "ok" and (baseline_dir / old["shot"]).is_file():
                row["baseline"] = scores(ours, load(baseline_dir / old["shot"]))
                if row["baseline"]["rmse"] > BASELINE_RMSE:
                    row["regressions"].append(f"changed: RMSE {row['baseline']['rmse']:.4f} against the baseline")
                old_score = old.get("reference", {}).get("score")
                new_score = row.get("reference", {}).get("score")
                if old_score is not None and new_score is not None and new_score < old_score - REFERENCE_LOSS:
                    row["regressions"].append(f"further from the reference: {old_score:.3f} -> {new_score:.3f}")
        if old and old.get("status") == "ok" and result["status"] != "ok":
            row["regressions"].append(f"was ok, now {result['status']}: {result['message']}")
        if old is None and baseline:
            row["new"] = True
        report[key] = row
    for key in baseline.keys() - results.keys():
        name = key.split("/")[0]
        if (data / "materials" / name).is_dir() and not subset:
            report[key] = {"status": "missing", "message": "in the baseline, not in this run", "regressions": ["missing"]}
    (run / "report.json").write_text(json.dumps(report, indent=1), encoding="utf-8")
    write_html(data, run, settings, results, report, baseline_dir if baseline else None)
    summarize(report, run)
    return report


def summarize(report, run):
    counts = {}
    for row in report.values():
        counts[row["status"]] = counts.get(row["status"], 0) + 1
    regressed = sorted(k for k, r in report.items() if r["regressions"])
    scored = [r["reference"]["score"] for r in report.values() if "reference" in r]
    print("status: " + ", ".join(f"{k} {v}" for k, v in sorted(counts.items())))
    if scored:
        print(f"reference score (pattern match): mean {np.mean(scored):.3f}, median {np.median(scored):.3f}")
    for key in regressed:
        print(f"REGRESSION {key}: {'; '.join(report[key]['regressions'])}")
    print(f"{len(regressed)} regressions; report: {(run / 'report.html').as_posix()}")


def write_html(data, run, settings, results, report, baseline_dir):
    def img(path):
        # Relative to the run directory, so the report also works served over HTTP from the data directory.
        uri = urllib.parse.quote(pathlib.Path(os.path.relpath(path, run)).as_posix())
        return f'<a href="{uri}"><img src="{uri}" loading="lazy"></a>'

    rows = []

    def order(key):
        row = report[key]
        return (not row["regressions"], row["status"] == "ok", row.get("reference", {}).get("score", 0), key)

    for key in sorted(report, key=order):
        row = report[key]
        name = key.split("/")[0]
        result = results.get(key, {})
        ref = data / "materials" / name / "preview.jpg"
        shot = run / result["shot"] if result.get("shot") else None
        cells = [img(ref) if ref.is_file() else "", img(shot) if shot and shot.is_file() else ""]
        if baseline_dir:
            old = baseline_dir / "shots" / pathlib.PurePosixPath(result.get("shot", "")).name
            cells.append(img(old) if result.get("shot") and old.is_file() else "")
        r, b = row.get("reference"), row.get("baseline")
        metrics = f"uv scale {result.get('uv_scale', 1.0)}<br>" + (f"ref match {r['score']:.3f} ssim {r['ssim']:.3f} rmse {r['rmse']:.3f} Δmean {r['mean_color']:.3f}<br>" if r else "") + (
            f"base rmse {b['rmse']:.4f} ssim {b['ssim']:.3f}" if b else ""
        )
        notes = "<br>".join(html.escape(x) for x in row["regressions"]) or html.escape(row["message"])
        cls = "bad" if row["regressions"] else ("err" if row["status"] != "ok" else "")
        rows.append(
            f'<tr class="{cls}"><td><b>{html.escape(key)}</b><br>{row["status"]}</td>'
            + "".join(f"<td>{c}</td>" for c in cells)
            + f"<td>{metrics}</td><td class=note>{notes}</td></tr>"
        )
    head = "<th>package</th><th>library preview</th><th>shadewire</th>" + (
        "<th>baseline</th>" if baseline_dir else ""
    )
    page = f"""<!doctype html><meta charset="utf-8"><title>matlib regression</title>
<style>
body{{font:13px system-ui,sans-serif;background:#1d1d1d;color:#ddd;margin:16px}}
table{{border-collapse:collapse}} td,th{{border-bottom:1px solid #333;padding:4px 8px;vertical-align:top;text-align:left}}
img{{width:160px;height:160px;display:block}} tr.bad{{background:#4a1f1f}} tr.err{{background:#3a3320}}
.note{{max-width:420px;word-break:break-word}}
</style>
<h1>matlib regression: {html.escape(run.name)}</h1>
<p>Renderer: {html.escape(settings.get("renderer", "bevy"))} {html.escape(" ".join(settings.get("renderer_args", [])))}</p>
<p>Sorted: regressions first, then failures, then by distance from the library preview (furthest first).</p>
<table><tr>{head}<th>metrics</th><th>notes</th></tr>
{''.join(rows)}
</table>"""
    (run / "report.html").write_text(page, encoding="utf-8")


# ------------------------------------------------------------------------------------------- calibrate

# The UV scales tried on the sphere: the library's previews tile each material at a scale of their own.
SCALES = (0.25, 0.5, 1.0, 2.0, 4.0)

# A best scale less than this ahead of the runner-up is unsure: plain materials, where the scale does not
# matter, and patterns the shading hides. Those get FALLBACK_SCALE, the scale most materials calibrate to.
UNSURE_MARGIN = 0.03
FALLBACK_SCALE = 0.5


# The middle PATCH x PATCH pixels of the preview are searched for in the middle SEARCH x SEARCH pixels of
# each shot. Picked against hand-checked materials: off-center patches see more of the sphere's curvature
# and highlights, and smaller patches too little of the pattern, and both chose worse.
PATCH = 48
SEARCH = 128


def detail(image):
    """Luminance with the shading taken out: each pixel against its neighborhood's mean and spread, so
    the pattern counts and the lighting (which never matches the previews) does not."""
    lum = image @ np.array([0.2126, 0.7152, 0.0722])
    mean = box(lum, 8)
    spread = np.sqrt(np.maximum(box(lum * lum, 8) - mean * mean, 0.0)) + 0.02
    return (lum - mean) / spread


def best_match(patch, image):
    """The highest normalized cross-correlation of `patch` at any position inside `image`."""
    ph, pw = patch.shape
    p = (patch - patch.mean()) / (patch.std() + 1e-9)
    corr = np.fft.irfft2(np.fft.rfft2(image) * np.fft.rfft2(p[::-1, ::-1], s=image.shape), s=image.shape)
    corr = corr[ph - 1 :, pw - 1 :]

    def windows(values):
        sat = np.pad(values, ((1, 0), (1, 0))).cumsum(0).cumsum(1)
        return sat[ph:, pw:] - sat[:-ph, pw:] - sat[ph:, :-pw] + sat[:-ph, :-pw]

    n = ph * pw
    mean = windows(image) / n
    std = np.sqrt(np.maximum(windows(image * image) / n - mean * mean, 1e-12))
    return float((corr / (n * std)).max())


def pattern_match(shot, reference):
    """How well the middle of the preview's pattern is found in the middle of the shot, wherever it lies:
    high when the pattern has the same size and look, whatever its position."""
    c = SIZE // 2
    patch = detail(reference)[c - PATCH // 2 : c + PATCH // 2, c - PATCH // 2 : c + PATCH // 2]
    region = detail(shot)[c - SEARCH // 2 : c + SEARCH // 2, c - SEARCH // 2 : c + SEARCH // 2]
    return best_match(patch, region)


def calibrate(data, only, renderer, renderer_args):
    """Renders each material's 1k 8-bit package at every scale of SCALES and keeps, in calibration.json,
    the scale whose shot is closest to the library's preview."""
    work = data / "calibration"
    todo = []
    for key, mtlx, name, _ in entries(data, only, ["1k_8b"]):
        for scale in SCALES:
            todo.append((f"{key}@{scale}", mtlx, name.replace(".png", f"@{scale}.png"), scale))
    if not todo:
        sys.exit(f"error: nothing to calibrate under {data / 'materials'}; run fetch.py first")
    # One work directory per renderer and its options, so a calibration resumes only its own shots.
    work = work / renderer / "_".join(a.replace("/", "-").replace(":", "") for a in renderer_args)[:80]
    results = render(work, todo, renderer, renderer_args)
    calibration = load_calibration(data)
    for key, mtlx, name, _ in entries(data, only, ["1k_8b"]):
        folder = key.split("/")[0]
        preview = data / "materials" / folder / "preview.jpg"
        if not preview.is_file():
            continue
        reference = load(preview)
        scores = {}
        for scale in SCALES:
            result = results[f"{key}@{scale}"]
            if result["status"] == "ok":
                shot = load(work / result["shot"])
                scores[scale] = pattern_match(shot, reference)
        if not scores:
            continue
        ranked = sorted(scores, key=scores.get, reverse=True)
        best = ranked[0]
        # How far the best scale is ahead of the runner-up: small margins are guesses worth a look.
        margin = scores[best] - scores[ranked[1]] if len(ranked) > 1 else 0.0
        uv_scale = best if margin >= UNSURE_MARGIN else FALLBACK_SCALE
        calibration[folder] = {
            "uv_scale": uv_scale,
            "renderer": " ".join([renderer, *renderer_args]),
            "best": best,
            "margin": margin,
            "scores": {str(k): v for k, v in scores.items()},
        }
        print(f"{folder}: uv scale {uv_scale} (best {best}, margin {margin:.3f})", flush=True)
    (data / "calibration.json").write_text(json.dumps(calibration, indent=1, sort_keys=True), encoding="utf-8")
    unsure = sorted(k for k, v in calibration.items() if v["margin"] < UNSURE_MARGIN)
    print(f"calibrated {len(calibration)} materials into {(data / 'calibration.json').as_posix()}")
    print(f"{len(unsure)} with a margin under {UNSURE_MARGIN} get {FALLBACK_SCALE}: {', '.join(unsure)}")


# ------------------------------------------------------------------------------------------------ main


def accept(data, run):
    settings = json.loads((run / "run.json").read_text(encoding="utf-8"))
    baseline = baseline_of(data, settings.get("renderer", "bevy"))
    report = json.loads((run / "report.json").read_text(encoding="utf-8"))
    results = json.loads((run / "results.json").read_text(encoding="utf-8"))
    old_path = baseline / "results.json"
    merged = json.loads(old_path.read_text(encoding="utf-8")) if old_path.is_file() else {}
    (baseline / "shots").mkdir(parents=True, exist_ok=True)
    for key, result in results.items():
        entry = dict(result)
        if "reference" in report.get(key, {}):
            entry["reference"] = report[key]["reference"]
        if result["status"] == "ok":
            shutil.copyfile(run / result["shot"], baseline / result["shot"])
        merged[key] = entry
    old_path.write_text(json.dumps(merged, indent=1), encoding="utf-8")
    print(f"accepted {len(results)} packages from {run} into {baseline}")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--data", type=pathlib.Path, default=DEFAULT_DATA)
    sub = parser.add_subparsers(dest="command", required=True)
    run_p = sub.add_parser("run", help="render, then compare")
    run_p.add_argument("--only", nargs="*", help="only these material folders")
    run_p.add_argument("--variants", nargs="*", default=list(VARIANTS), choices=VARIANTS)
    run_p.add_argument("--out", type=pathlib.Path, help="run directory (default <data>/runs/<timestamp>)")
    cal_p = sub.add_parser("calibrate", help="pick each material's sphere UV scale against the previews")
    cal_p.add_argument("--only", nargs="*", help="only these material folders")
    for p in (run_p, cal_p):
        p.add_argument("--renderer", choices=sorted(RENDERERS), default=DEFAULT_RENDERER)
        p.add_argument("--renderer-args", default="", help="the renderer's options as one string (replace its defaults)")
    for name in ("compare", "accept"):
        p = sub.add_parser(name)
        p.add_argument("run", type=pathlib.Path)
    args = parser.parse_args()
    data = args.data

    if args.command == "run":
        run = args.out or data / "runs" / datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
        todo = entries(data, set(args.only or []), args.variants)
        if not todo:
            sys.exit(f"error: nothing to render under {data / 'materials'}; run fetch.py first")
        run.mkdir(parents=True, exist_ok=True)
        options = renderer_args(data, args.renderer, args.renderer_args)
        settings = {"only": args.only, "variants": args.variants, "renderer": args.renderer}
        settings["renderer_args"] = options
        (run / "run.json").write_text(json.dumps(settings), "utf-8")
        results = render(run, todo, args.renderer, options)
        (run / "results.json").write_text(json.dumps(results, indent=1), encoding="utf-8")
        report = compare(data, run)
    elif args.command == "calibrate":
        calibrate(data, set(args.only or []), args.renderer, renderer_args(data, args.renderer, args.renderer_args))
        return 0
    elif args.command == "compare":
        report = compare(data, args.run)
    else:
        accept(data, args.run)
        return 0
    return 1 if any(r["regressions"] for r in report.values()) else 0


if __name__ == "__main__":
    sys.exit(main())
