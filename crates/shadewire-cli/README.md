# shadewire-cli

The `shadewire` command of [Shadewire](https://github.com/rdeioris/shadewire): compiles MaterialX materials and Blender material graphs to WGSL,
GLSL, GLSL ES, HLSL or MSL with their reflection, prints a material's IR, lists a document's materials.

```
cargo install shadewire-cli
shadewire compile material.mtlx --lang glsl --reflection material.reflection.json -o material.glsl
shadewire compile material.json --lang wgsl
shadewire ir material.mtlx --json
shadewire list scene.mtlx
```

Blender graphs (`.json`, from the Blender extension's File > Export > Material Graph) are read wherever a `.mtlx` is;
`--format` overrides the extension. What the Blender front end dropped or approximated prints to stderr, one line each.

Part of [Shadewire](https://github.com/rdeioris/shadewire), a compiler for material graphs: a front end turns a material description into a typed
IR, back ends turn it into shader code plus reflection that says what the host must bind.
