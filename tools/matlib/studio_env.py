"""Writes the studio environment map the harness lights the path tracer with: an equirectangular Radiance
.hdr of a dark room with soft round lights, approximating the lighting of the library's previews (a large
key light above and left of the camera, a rim from above and behind, a surround brighter overhead).

    python -I tools/matlib/studio_env.py <out.hdr> [--width 1024] [--key K] [--ambient A]

Directions follow the path tracer's convention (Y up, camera on +Z looking at the origin): u = atan2(x, -z)
/ 2pi + 0.5 and v = acos(y) / pi, so -Z is the middle of the map.
"""
import argparse
import pathlib
import sys

import numpy as np

# Radiance of the key light and of the surround: with the harness's exposure, the previews' brightness.
KEY = 8.0
AMBIENT = 0.15

# (direction toward the light, angular radius in degrees, radiance relative to the key)
LIGHTS = (
    ((-0.55, 0.65, 0.55), 32.0, 1.0),  # key: above left, in front
    ((0.0, 0.8, -0.6), 22.0, 0.35),  # rim: above, behind
)


def environment(width, key, ambient):
    """The map as linear RGB floats, height x width x 3."""
    height = width // 2
    v, u = np.mgrid[0:height, 0:width]
    phi = ((u + 0.5) / width - 0.5) * 2.0 * np.pi
    theta = (v + 0.5) / height * np.pi
    d = np.stack([np.sin(theta) * np.sin(phi), np.cos(theta), -np.sin(theta) * np.cos(phi)], axis=-1)
    # A dim surround, brighter overhead than below like a room's ceiling and floor.
    radiance = ambient * (0.3 + 0.7 * np.clip(0.5 + 0.5 * d[..., 1], 0.0, 1.0) ** 2 * 2.0)
    for direction, radius, weight in LIGHTS:
        axis = np.array(direction) / np.linalg.norm(direction)
        angle = np.degrees(np.arccos(np.clip(d @ axis, -1.0, 1.0)))
        # A softbox: flat inside, smooth falloff over its outer fifth.
        edge = np.clip((radius - angle) / (0.2 * radius), 0.0, 1.0)
        radiance = radiance + key * weight * edge * edge * (3.0 - 2.0 * edge)
    return np.repeat(radiance[..., None], 3, axis=-1).astype(np.float32)


def write_hdr(path, image):
    """A Radiance RGBE file with flat (uncompressed) scanlines."""
    height, width, _ = image.shape
    peak = image.max(axis=-1)
    mantissa, exponent = np.frexp(peak)
    scale = np.where(peak > 1e-32, mantissa * 256.0 / np.maximum(peak, 1e-32), 0.0)
    rgbe = np.zeros((height, width, 4), dtype=np.uint8)
    rgbe[..., :3] = np.clip(image * scale[..., None], 0, 255).astype(np.uint8)
    rgbe[..., 3] = np.where(peak > 1e-32, exponent + 128, 0).astype(np.uint8)
    header = f"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y {height} +X {width}\n".encode("ascii")
    path.write_bytes(header + rgbe.tobytes())


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("out", type=pathlib.Path)
    parser.add_argument("--width", type=int, default=1024)
    parser.add_argument("--key", type=float, default=KEY, help="radiance of the key light")
    parser.add_argument("--ambient", type=float, default=AMBIENT, help="radiance of the surround")
    args = parser.parse_args()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    write_hdr(args.out, environment(args.width, args.key, args.ambient))
    print(f"wrote {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
