These are `shadewire-blender-graph` documents dumped by the Blender add-on from ten materials built in `blender/make_fixtures.py`, one theme each (Principled defaults, every Principled input, textures, math, procedural textures, lobes, structure, unsupported nodes, displacement and volume, the glTF Material Output group).
Regenerate them with `blender -b --python-exit-code 1 -P blender/make_fixtures.py` on the Blender the Rust tests target; node defaults and enums differ between versions. `blender -b --python-exit-code 1 -P blender/test_graph.py` fails when they are stale.
The Rust front end's tests (`crates/shadewire-blender`) read them as the real input of the Blender graph format.

What Blender writes that a reader may not expect:

- The Glossy BSDF's `type` is `ShaderNodeBsdfAnisotropic`: Blender kept the old `bl_idname` when it merged the two nodes.
- Group Input and Group Output nodes carry a last `__extend__` socket, Blender's virtual socket for adding interface sockets: type `CUSTOM`, no value.
- `internal_links` is filled on unmuted nodes too: it lists the links Blender would pass through if the node were muted.
