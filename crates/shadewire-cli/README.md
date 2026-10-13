# shadewire-cli

The `shadewire` command of [Shadewire](https://github.com/rdeioris/shadewire): compiles MaterialX materials to WGSL, GLSL, GLSL ES, HLSL or MSL
with their reflection, prints a material's IR, lists a document's materials.

```
cargo install shadewire-cli
shadewire compile material.mtlx --lang glsl --reflection material.json -o material.glsl
shadewire ir material.mtlx --json
shadewire list scene.mtlx
```

Part of [Shadewire](https://github.com/rdeioris/shadewire), a compiler for material graphs: a front end turns a material description into a typed
IR, back ends turn it into shader code plus reflection that says what the host must bind.
