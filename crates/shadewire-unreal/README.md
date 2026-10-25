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
- **Substrate**: the Front Material's tree becomes a closure tree (host must accept `surface`). A Slab is a
  Schlick specular lobe (F0, F90, roughness, anisotropy) layered over a diffuse lobe of its Diffuse Albedo, or a
  subsurface lobe when it has an SSS MFP; Second Roughness mixes in a second specular lobe, Fuzz layers a sheen lobe
  on top, Emissive Color is a uniform EDF. Simple Clear Coat is a dielectric coat over a slab, Unlit an EDF.
  Horizontal Mixing is `mix`, Vertical Layering `layer` (the top's medium is not read: a top layer is its specular
  and fuzz), Add `add`, Weight `multiply`, Select a 0-or-1 `mix`. A Substrate Shading Models node on the Front
  Material reads as the legacy pins (`open_pbr_surface`); below an operator it becomes a slab by Unreal's metalness
  conversion. Metalness-To-DiffuseAlbedo-F0, Haziness-To-Secondary-Roughness and Transmittance-To-MFP compute what
  Unreal's shaders compute; glints, specular and subsurface profiles and thin-film interference are dropped with a
  warning.
- **Engine-only expressions** (Custom HLSL, scene textures, depths) and anything not mapped yet are a warning and a
  neutral value: `Custom "MaterialExpressionCustom_0": Custom HLSL replaced by 0`.
