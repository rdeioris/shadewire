# shadewire-blender

The Blender front end of [Shadewire](https://github.com/rdeioris/shadewire): reads the `shadewire-blender-graph` JSON
document the Blender extension writes (File > Export > Material Graph (Shadewire JSON)) and lowers one material to
Shadewire's IR through `shadewire-nodes`: a Principled BSDF becomes `open_pbr_surface`, Blender's lobes a closure tree,
its math, color, texture and procedural nodes the matching MaterialX nodes.
