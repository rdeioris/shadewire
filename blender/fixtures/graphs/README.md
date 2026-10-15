These are `shadewire-blender-graph` documents dumped by the Blender add-on from eight materials built in `blender/make_fixtures.py`, one theme each (Principled defaults, every Principled input, textures, math, procedural textures, lobes, structure, unsupported nodes).
Regenerate them with `blender -b --python-exit-code 1 -P blender/make_fixtures.py`; `blender -b --python-exit-code 1 -P blender/test_graph.py` fails when they are stale (only the `producer` line may differ).
The Rust front end's tests (`crates/shadewire-blender`) read them as the real input of the Blender graph format.
