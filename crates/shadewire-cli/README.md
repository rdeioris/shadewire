# shadewire-cli

The `shadewire` command of [Shadewire](https://github.com/rdeioris/shadewire): compiles MaterialX materials, Blender material graphs and NVIDIA MDL
modules to WGSL, GLSL, GLSL ES, HLSL, MSL, Godot shaders or MDL with their reflection, prints a material's IR, lists a
document's materials.

```
cargo install shadewire-cli
shadewire compile material.mtlx --lang glsl --reflection material.reflection.json -o material.glsl
shadewire compile material.json --lang wgsl
shadewire compile material.mdl --lobes --mdl-path ./mdl --lang wgsl
shadewire compile material.mtlx --lang mdl -o material.mdl
shadewire ir material.mtlx --json
shadewire list scene.mtlx
```

Blender graphs (`.json`, from the Blender extension's File > Export > Material Graph) and MDL modules (`.mdl`) are read
wherever a `.mtlx` is; `--format` overrides the extension. An MDL material is a closure tree, so it needs `--lobes`;
`--mdl-path DIR` (repeatable) is where `import` finds modules. What a front end dropped or approximated prints to
stderr, one line each. `--lang mdl` writes a self-contained MDL 1.7 module and opens every material into its closure
tree by default.

Part of [Shadewire](https://github.com/rdeioris/shadewire), a compiler for material graphs: a front end turns a material description into a typed
IR, back ends turn it into shader code plus reflection that says what the host must bind.
