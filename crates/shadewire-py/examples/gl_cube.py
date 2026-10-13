"""A spinning cube with a MaterialX material in a pygame window, drawn with OpenGL.

    pip install shadewire pygame PyOpenGL pillow
    python gl_cube.py material.mtlx [--material NAME] [--size 800] [--frames N --screenshot out.png]

The shader is the one of `wgpu_cube.py` (Shadewire's module, the host of `wgsl_lobes.py`, a vertex stage), linked
as WGSL and translated by Shadewire to GLSL 4.20 core, one program per stage. GLSL has no bind groups:
`@group(g) @binding(b)` becomes binding point `32 * g + b` (`shadewire.wgsl.translate`), so the scene's uniform
blocks sit at 0 and 1 and the material's resources from 32 on, and each texture combines with its sampler into
one `sampler2D` at the texture's binding point. Escape or closing the window quits; the camera orbits.
"""

import argparse
import ctypes
import os
import sys

import pygame
from OpenGL import GL
from OpenGL.GL.shaders import compileProgram, compileShader
from PIL import Image

import scene
import shadewire
from shadewire import wgsl

GLSL_VERSION = 420
WRAP = {"periodic": GL.GL_REPEAT, "mirror": GL.GL_MIRRORED_REPEAT, "clamp": GL.GL_CLAMP_TO_EDGE,
        "constant": GL.GL_CLAMP_TO_EDGE}


def gl_binding(group, binding):
    """Where Shadewire's GLSL puts WGSL `@group(group) @binding(binding)`."""
    return 32 * group + binding


def program(linked):
    """The linked WGSL as a GLSL program, Shadewire writing each stage."""
    stages = []
    for entry, kind in (("vs_main", GL.GL_VERTEX_SHADER), ("fs_main", GL.GL_FRAGMENT_SHADER)):
        source = wgsl.translate(linked, "glsl", GLSL_VERSION, entry_point=entry)
        stages.append(compileShader(source, kind))
    return compileProgram(*stages, validate=False)


def uniform_buffer(data, point):
    buffer = GL.glGenBuffers(1)
    GL.glBindBuffer(GL.GL_UNIFORM_BUFFER, buffer)
    GL.glBufferData(GL.GL_UNIFORM_BUFFER, len(data), data, GL.GL_DYNAMIC_DRAW)
    GL.glBindBufferBase(GL.GL_UNIFORM_BUFFER, point, buffer)
    return buffer


def bind_material(reflection, base_dir):
    """The material's uniform block and textures at the binding points of group `scene.MATERIAL_GROUP`."""
    group = scene.MATERIAL_GROUP
    if reflection["uniform_binding"] is not None:
        uniform_buffer(scene.uniform_bytes(reflection), gl_binding(group, reflection["uniform_binding"]))
    for t in reflection["textures"]:
        path = os.path.join(base_dir, t["file"])
        try:
            image = Image.open(path).convert("RGBA")
        except OSError:
            print(f"warning: texture {path} not found; it reads as mid grey")
            image = Image.new("RGBA", (1, 1), (128, 128, 128, 255))
        GL.glActiveTexture(GL.GL_TEXTURE0 + gl_binding(group, t["binding"]))
        texture = GL.glGenTextures(1)
        GL.glBindTexture(GL.GL_TEXTURE_2D, texture)
        # Raw texels (GL_RGBA8, never GL_SRGB8_ALPHA8: the generated code converts colorspaces), top row first:
        # Shadewire flips V when sampling, as for wgpu.
        GL.glTexImage2D(GL.GL_TEXTURE_2D, 0, GL.GL_RGBA8, image.width, image.height, 0, GL.GL_RGBA,
                        GL.GL_UNSIGNED_BYTE, image.tobytes())
        GL.glGenerateMipmap(GL.GL_TEXTURE_2D)
        address_u, address_v = t["address"]
        GL.glTexParameteri(GL.GL_TEXTURE_2D, GL.GL_TEXTURE_WRAP_S, WRAP[address_u])
        GL.glTexParameteri(GL.GL_TEXTURE_2D, GL.GL_TEXTURE_WRAP_T, WRAP[address_v])
        nearest = t["filter"] == "closest"
        GL.glTexParameteri(GL.GL_TEXTURE_2D, GL.GL_TEXTURE_MAG_FILTER, GL.GL_NEAREST if nearest else GL.GL_LINEAR)
        GL.glTexParameteri(GL.GL_TEXTURE_2D, GL.GL_TEXTURE_MIN_FILTER,
                           GL.GL_NEAREST_MIPMAP_NEAREST if nearest else GL.GL_LINEAR_MIPMAP_LINEAR)


def cube_vao():
    vertices, indices, count = scene.cube()
    vao = GL.glGenVertexArrays(1)
    GL.glBindVertexArray(vao)
    GL.glBindBuffer(GL.GL_ARRAY_BUFFER, GL.glGenBuffers(1))
    GL.glBufferData(GL.GL_ARRAY_BUFFER, len(vertices), vertices, GL.GL_STATIC_DRAW)
    GL.glBindBuffer(GL.GL_ELEMENT_ARRAY_BUFFER, GL.glGenBuffers(1))
    GL.glBufferData(GL.GL_ELEMENT_ARRAY_BUFFER, len(indices), indices, GL.GL_STATIC_DRAW)
    for location, floats, offset in scene.VERTEX_ATTRIBUTES:
        GL.glEnableVertexAttribArray(location)
        GL.glVertexAttribPointer(location, floats, GL.GL_FLOAT, GL.GL_FALSE, scene.VERTEX_STRIDE, ctypes.c_void_p(offset))
    return vao, count


def open_window(size):
    pygame.init()
    for attribute, value in ((pygame.GL_CONTEXT_MAJOR_VERSION, 4), (pygame.GL_CONTEXT_MINOR_VERSION, 2),
                             (pygame.GL_CONTEXT_PROFILE_MASK, pygame.GL_CONTEXT_PROFILE_CORE),
                             (pygame.GL_MULTISAMPLEBUFFERS, 1), (pygame.GL_MULTISAMPLESAMPLES, 4),
                             (pygame.GL_FRAMEBUFFER_SRGB_CAPABLE, 1), (pygame.GL_DEPTH_SIZE, 24)):
        pygame.display.gl_set_attribute(attribute, value)
    pygame.display.set_mode((size, size), pygame.OPENGL | pygame.DOUBLEBUF)
    pygame.display.set_caption("Shadewire MaterialX cube (OpenGL)")
    print("OpenGL", GL.glGetString(GL.GL_VERSION).decode(), "on", GL.glGetString(GL.GL_RENDERER).decode())


def screenshot(size, path):
    GL.glReadBuffer(GL.GL_BACK)
    pixels = GL.glReadPixels(0, 0, size, size, GL.GL_RGB, GL.GL_UNSIGNED_BYTE)
    # OpenGL's rows start at the bottom.
    Image.frombytes("RGB", (size, size), pixels).transpose(Image.Transpose.FLIP_TOP_BOTTOM).save(path)
    print("wrote", path)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("mtlx")
    parser.add_argument("--material")
    parser.add_argument("--size", type=int, default=800)
    parser.add_argument("--frames", type=int, help="draw this many frames, then quit")
    parser.add_argument("--screenshot", help="save the last frame here")
    args = parser.parse_args()

    try:
        compiled, linked = scene.compile_material(args.mtlx, args.material)
    except shadewire.ShadewireError as e:
        sys.exit(f"shadewire: {e}")
    open_window(args.size)
    shader = program(linked)
    vao, index_count = cube_vao()
    angle = 0.6
    eye = scene.orbit(angle)
    scene_buffer = uniform_buffer(scene.scene_bytes(eye), gl_binding(0, 0))
    camera_buffer = uniform_buffer(scene.camera_bytes(eye, 1.0), gl_binding(0, 1))
    bind_material(compiled.reflection, os.path.dirname(args.mtlx))

    GL.glEnable(GL.GL_DEPTH_TEST)
    GL.glEnable(GL.GL_CULL_FACE)
    GL.glEnable(GL.GL_MULTISAMPLE)
    GL.glEnable(GL.GL_FRAMEBUFFER_SRGB)  # the shader writes linear color
    clock = pygame.time.Clock()
    frame = 0
    running = True
    while running:
        for event in pygame.event.get():
            if event.type == pygame.QUIT or (event.type == pygame.KEYDOWN and event.key == pygame.K_ESCAPE):
                running = False
        angle += clock.tick(60) / 1000.0 * 0.5
        eye = scene.orbit(angle)
        for buffer, data in ((scene_buffer, scene.scene_bytes(eye)), (camera_buffer, scene.camera_bytes(eye, 1.0))):
            GL.glBindBuffer(GL.GL_UNIFORM_BUFFER, buffer)
            GL.glBufferSubData(GL.GL_UNIFORM_BUFFER, 0, len(data), data)
        GL.glClearColor(0.08, 0.08, 0.09, 1.0)
        GL.glClear(GL.GL_COLOR_BUFFER_BIT | GL.GL_DEPTH_BUFFER_BIT)
        GL.glUseProgram(shader)
        GL.glBindVertexArray(vao)
        GL.glDrawElements(GL.GL_TRIANGLES, index_count, GL.GL_UNSIGNED_INT, None)
        frame += 1
        if args.frames and frame >= args.frames:
            if args.screenshot:
                screenshot(args.size, args.screenshot)
            running = False
        pygame.display.flip()
    pygame.quit()


if __name__ == "__main__":
    main()
