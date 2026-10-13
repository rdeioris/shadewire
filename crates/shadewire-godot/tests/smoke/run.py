"""Smoke-test the Shadewire extension in Godot with the real renderer.

Run from anywhere, with a Godot 4.4+ binary (the console build on Windows) and a GPU:
    python -I crates/shadewire-godot/tests/smoke/run.py <path to godot> [screenshot.png]

It builds the extension, assembles a project in crates/shadewire-godot/target/smoke (this folder, the addon with
the library, and the vendored grid.png), imports it with the editor (the importer runs), then runs smoke.gd, which
loads materials through the importer, the runtime API and the resource loader, draws them and saves a screenshot
(default target/smoke/smoke.png). Exits 1 on a failed check or any shader error.

For load_material_async it serves a remote document from two local HTTP servers, host A (127.0.0.1) and host B
(localhost), on free ports, over plain http (smoke.gd passes the allow_http option). The document on A has a
relative include and a relative texture, names a texture on B, and names one on A that redirects to B. smoke.gd
sends an auth header scoped to A; the servers log every request, and this script checks what reached each host.
"""
import http.server
import json
import pathlib
import shutil
import socket
import struct
import subprocess
import sys
import threading
import zlib

HERE = pathlib.Path(__file__).resolve().parent
CRATE = HERE.parent.parent
ROOT = CRATE.parent.parent
EXAMPLES = ROOT / "vendor" / "materialx" / "resources" / "Materials" / "Examples"
VENDORED = EXAMPLES / "StandardSurface" / "standard_surface_marble_solid.mtlx"
GRID = ROOT / "vendor" / "materialx" / "resources" / "Images" / "grid.png"
TOKEN = "Bearer smoke-token"

REMOTE_DOCUMENT = """<?xml version="1.0"?>
<materialx version="1.39">
  <xi:include href="lib/remote_graph.mtlx" />
  <standard_surface name="SR_remote" type="surfaceshader">
    <input name="base_color" type="color3" nodegraph="NG_remote" output="out" />
    <input name="specular_roughness" type="float" value="0.35" />
  </standard_surface>
  <surfacematerial name="M_remote" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="SR_remote" />
  </surfacematerial>
</materialx>
"""

# Texture files resolve against the top document (/m/), the include only against its own location.
REMOTE_GRAPH = """<?xml version="1.0"?>
<materialx version="1.39">
  <nodegraph name="NG_remote">
    <image name="grid" type="color3">
      <input name="file" type="filename" value="tex/grid.png" colorspace="srgb_texture" />
    </image>
    <image name="other_host" type="color3">
      <input name="file" type="filename" value="{b}/t/white.png" colorspace="srgb_texture" />
    </image>
    <image name="redirected" type="color3">
      <input name="file" type="filename" value="tex/redirect.png" colorspace="srgb_texture" />
    </image>
    <multiply name="m1" type="color3">
      <input name="in1" type="color3" nodename="grid" />
      <input name="in2" type="color3" nodename="other_host" />
    </multiply>
    <multiply name="m2" type="color3">
      <input name="in1" type="color3" nodename="m1" />
      <input name="in2" type="color3" nodename="redirected" />
    </multiply>
    <output name="out" type="color3" nodename="m2" />
  </nodegraph>
</materialx>
"""


def white_png(size=4):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    rows = b"".join(b"\0" + b"\xff\xff\xff" * size for _ in range(size))
    header = struct.pack(">IIBBBBB", size, size, 8, 2, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")


class Server:
    """A static file server on a free port of `hosts` (one port, every address), logging each request."""

    def __init__(self, name, root, hosts, redirects):
        self.log = []
        server = self

        class Handler(http.server.SimpleHTTPRequestHandler):
            def __init__(self, *args, **kwargs):
                super().__init__(*args, directory=str(root), **kwargs)

            def do_GET(self):
                if self.path in redirects:
                    self.send_response(302)
                    self.send_header("Location", redirects[self.path])
                    self.send_header("Content-Length", "0")
                    self.end_headers()
                    return
                super().do_GET()

            def log_request(self, code="-", size="-"):
                server.log.append({
                    "server": name,
                    "path": self.path,
                    "status": int(code),
                    "authorization": self.headers.get("Authorization"),
                    "phase": self.headers.get("X-Smoke-Phase"),
                    "signed": self.headers.get("X-Signed"),
                })

        self.servers = []
        port = 0
        for host in hosts:
            family = socket.AF_INET6 if ":" in host else socket.AF_INET
            cls = type("S", (http.server.ThreadingHTTPServer,), {"address_family": family})
            httpd = cls((host, port), Handler)
            port = httpd.server_address[1]
            self.servers.append(httpd)
            threading.Thread(target=httpd.serve_forever, daemon=True).start()
        self.port = port

    def close(self):
        for httpd in self.servers:
            httpd.shutdown()
            httpd.server_close()


def serve(work):
    """Host A at 127.0.0.1 and host B at localhost (IPv4 and IPv6, whichever Godot resolves)."""
    a_root, b_root = work / "http_a", work / "http_b"
    (a_root / "m" / "lib").mkdir(parents=True)
    (a_root / "m" / "tex").mkdir()
    (b_root / "t").mkdir(parents=True)
    shutil.copy(GRID, a_root / "m" / "tex" / "grid.png")
    (b_root / "t" / "white.png").write_bytes(white_png())
    (b_root / "t" / "redirected.png").write_bytes(white_png(8))
    b_hosts = ["127.0.0.1"]
    try:
        socket.getaddrinfo("::1", 0, socket.AF_INET6)
        b_hosts.append("::1")
    except OSError:
        pass
    b = Server("b", b_root, b_hosts, {})
    b_url = f"http://localhost:{b.port}"
    a = Server("a", a_root, ["127.0.0.1"], {"/m/tex/redirect.png": f"{b_url}/t/redirected.png"})
    (a_root / "m" / "remote.mtlx").write_text(REMOTE_DOCUMENT)
    (a_root / "m" / "lib" / "remote_graph.mtlx").write_text(REMOTE_GRAPH.format(b=b_url))
    return a, b, f"http://127.0.0.1:{a.port}", b_url


def check_requests(log):
    """The four remote checks the servers can see; returns failure messages."""
    failures = []

    def expect(ok, what):
        print(("OK   " if ok else "FAIL ") + "server: " + what)
        if not ok:
            failures.append(what)

    a = [r for r in log if r["server"] == "a"]
    b = [r for r in log if r["server"] == "b"]
    phase = lambda n: [r for r in a if r["phase"] == str(n)]
    expect(a and all(r["authorization"] == TOKEN for r in a), f"the auth header reached every request to A ({len(a)})")
    expect(b and not any(r["authorization"] for r in b), f"no request to B carried it ({len(b)})")
    expect(not any(r["phase"] for r in b), "per-request headers stayed on the document's host")
    expect({r["path"] for r in b} == {"/t/white.png", "/t/redirected.png"}, "B served the named and the redirected texture")
    first = {r["path"]: r["status"] for r in phase(1)}
    expect(first == {"/m/remote.mtlx": 200, "/m/lib/remote_graph.mtlx": 200, "/m/tex/grid.png": 200,
                     "/m/tex/redirect.png": 302}, f"first load fetched everything: {first}")
    signed = {}
    for r in phase(1) + b:  # B's first requests are the first load's
        signed.setdefault(r["path"], r["signed"])
    expect(signed.get("/m/remote.mtlx") == "remote.mtlx" and signed.get("/t/white.png") == "white.png",
           "http_headers_callback headers were sent")
    second = {r["path"]: r["status"] for r in phase(2)}
    expect(second == {"/m/remote.mtlx": 304, "/m/lib/remote_graph.mtlx": 304, "/m/tex/grid.png": 304,
                      "/m/tex/redirect.png": 302}, f"second load revalidated the cache: {second}")
    b_ok = [r["path"] for r in b if r["status"] == 200]
    expect(sorted(b_ok) == ["/t/redirected.png", "/t/white.png"] and len(b) == 4,
           f"B served each texture once, then 304s ({[(r['path'], r['status']) for r in b]})")
    expect(not phase(3), "a cache_policy \"use\" load made no request")
    return failures


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
    shutil.copy(GRID, work / "smoke" / "textures")

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

    # Outside the project folder, so the editor never imports the served files.
    http_work = CRATE / "target" / "smoke_http"
    shutil.rmtree(http_work, ignore_errors=True)
    a, b, a_url, b_url = serve(http_work)
    try:
        cache = (http_work / "cache").as_posix()
        user = [VENDORED.as_posix(), str(screenshot), a_url, b_url, cache, TOKEN]
        code, log = godot(binary, work, "--resolution", "1200x300", "--script", "res://smoke.gd", "--", *user)
    finally:
        a.close()
        b.close()
    print(log)
    requests = a.log + b.log
    (http_work / "requests.json").write_text(json.dumps(requests, indent=1))
    failures = check_requests(requests)
    errors = [line for line in log.splitlines() if "SHADER ERROR" in line or "ERROR:" in line]
    if code != 0 or errors or "FAILURES 0" not in log or failures:
        sys.exit(f"smoke test failed (exit {code}, {len(errors)} error lines, {len(failures)} server checks)")
    print(f"smoke test passed; screenshot {screenshot}")


if __name__ == "__main__":
    main()
