# shadewire-unreal

The Unreal Engine front end of [Shadewire](https://github.com/rdeioris/shadewire): reads the `shadewire-unreal-graph`
JSON document the Unreal plugin writes (`unreal/Shadewire`: Save Shadewire Graph, or its commandlet with
`-format=json`) and lowers one material to Shadewire's IR through `shadewire-nodes`. A material in a legacy shading
model becomes `open_pbr_surface` (Unlit becomes `surface_unlit`); its expressions become the matching MaterialX nodes.

The document holds materials as the editor has them: every expression by class name with its properties, its inputs
by name with what they link to and their masks, the material's pins, the parents of an instance with their overrides,
the material functions it calls and the textures it reads.

```rust
use shadewire_unreal::{IrOptions, to_ir};

let text = std::fs::read_to_string("M_Wood.json")?;
let ir = to_ir(&text, None, &IrOptions::default())?;
print!("{}", ir.dag.dump());
for warning in &ir.warnings {
    eprintln!("{warning}");
}
```

## What maps

- **Instances**: a material instance reads its parent chain; the nearest instance's override of a parameter wins,
  then its parents', then the parameter's default. Scalar and vector parameters become uniforms named after the
  parameter; static switches pick their branch.
- **Functions** are inlined per call; a function input reads what the call links to it, else its preview when that
  is the default. **Reroutes** and named reroutes are followed. **Quality, feature level and shading path switches**
  take the Default (or High), SM6 and Deferred branches, with a warning naming the branches dropped.
- **Masks**: each link's component mask is applied to what it reads, as Unreal applies it.
- **Coordinates**: Unreal is left-handed, Z up, in centimeters, with V running down its textures; world positions,
  normals and the camera vector are converted from Shadewire's world (Y negated, meters), UVs flip V, tangent-space
  normals negate Y (Unreal's normal maps are green-down).
- **Surface**: Base Color, Metallic, Specular (`specular_weight` = 2 × Specular), Roughness, Anisotropy, Normal,
  Tangent, Emissive, Opacity (Translucent) and Opacity Mask against the clip value (Masked); Clear Coat, Subsurface,
  Cloth, Two Sided Foliage and Thin Translucent add their OpenPBR fields. Unlinked pins take Unreal's defaults.
- **Engine-only expressions** (Custom HLSL, scene textures, depths) and anything not mapped yet are a warning and a
  neutral value: `Custom "MaterialExpressionCustom_0": Custom HLSL replaced by 0`.

Substrate materials are not read yet.
