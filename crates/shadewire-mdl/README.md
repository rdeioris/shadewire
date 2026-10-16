# shadewire-mdl

NVIDIA MDL for [Shadewire](https://github.com/rdeioris/shadewire), both ways: the front end reads an MDL module and
lowers one of its materials to Shadewire's IR; the back end writes any IR with a closure tree as a self-contained
MDL 1.7 module that MDL renderers (Omniverse RTX, Iray, the MDL SDK) load with nothing else installed.

```rust
use shadewire::LOBE_MODELS;
use shadewire_mdl::{IrOptions, MdlOptions, Resolver, compile, to_ir};

let text = std::fs::read_to_string("material.mdl")?;
let resolver = Resolver::new(vec!["mdl".into()]); // the MDL search path
let ir = to_ir(&text, Some("material.mdl"), None, &IrOptions::with_models(&LOBE_MODELS), &resolver)?;
for warning in &ir.warnings {
    eprintln!("{warning}");
}

let module = compile(&ir.dag, &ir.source, &MdlOptions { name: "wood".to_string(), ..Default::default() })?;
print!("{}", module.code); // mdl 1.7; ... export material wood(...) = let { ... } in material(...);
```

## Reading

The reader is a compile-time evaluator, not a node mapper: it parses the module (MDL 1.0 to 1.11, with
[pest](https://pest.rs)) and runs the chosen material symbolically. Known values fold, values that vary per
fragment become IR nodes, calls are inlined, a branch on a run-time condition runs both sides under a mask, loops
unroll (at most 1024 iterations, then `Error::Limit`). Overloads are picked by the module's version and the cost of
their conversions, as the MDL SDK's compiler does, including the lenient conversions MDL in the wild relies on.

- `to_ir(text, location, material, &IrOptions, &Resolver)` lowers `material` (the module's only exported one when
  `None`); `material_names(text, location, &Resolver)` lists the exported materials.
- An MDL material is a closure tree: the `::df` tree maps onto MaterialX's pbrlib lobes, so `IrOptions` must
  accept `surface` (`LOBE_MODELS`); otherwise `to_ir` is `Error::NoTranslation`, as for any other front end.
- The material's parameters become uniforms named after them, with `anno::hard_range`, `soft_range` and `in_group`
  as UI hints; integer parameters stay constants (they are counts and modes). A parameter without a default is an
  error.
- `import` finds a module in the `Resolver`'s directories (a relative import, `.::` or `..::`, next to the importing
  module), then in the vendored set. The standard modules are always the vendored ones: NVIDIA's declarations, their
  meaning in Rust. `::base` and `::nvidia::core_definitions` are NVIDIA's source, evaluated; a module of the same
  name on the search path wins over them.
- `::base`'s noise primitives index permutation tables at run time; they are MaterialX's noises instead, with one
  warning per noise kind.

What the reader drops or approximates is in `MaterialIr.warnings`, one line each, as `{module}::{material}: {what}`:
non-GGX microfacet lobes, colored Fresnel IORs (averaged), texel reads and texture sizes, transforms between
spaces other than internal, object and world, measured data, volumes, backfaces, hair and displacement (dropped).
The source is `SourceInfo { source: "mdl", mtlx_version: "MDL 1.7" }`, with the module's version.

## Writing

`compile(&Dag, &SourceInfo, &MdlOptions)` prints the shared shader tree every back end prints, in MDL: the
patterns and Shadewire's support libraries (noise, color, hextile) as MDL functions, the closure tree as `::df`
calls in the material's `let`, uniforms as annotated material parameters, textures as `uniform texture_2d`
parameters (`tex::gamma_linear`: the code converts colorspaces itself). It imports only standard modules and writes
`mdl 1.7;`. `MdlOptions { name, optimize }` default to `material` and optimizing first.

The writer takes closure trees (`surface`) and `surface_unlit`; a named model is `Error::NoTranslation`. The lobes
follow MaterialX's own `pbrlib` MDL; what MDL 1.7 cannot say is listed in `reflection.approximations`: the F82 tint
of a Schlick lobe (folded into its grazing color), subsurface (diffuse reflection), Schlick and conical EDFs,
hair, the view direction (MDL has none for surfaces) and geometric properties (their defaults). A texture path is
written as it is, without a drive letter. `mtlx → ir → mdl → ir` gives back the same lobes and values except
there.

## Testing

`cargo test -p shadewire-mdl` parses every vendored module, evaluates small modules construct by construct, lowers
the fixtures in `tests/fixtures` (closure dumps and warnings are `insta` snapshots), writes every vendored MaterialX
example as MDL and reads it back (`tests/roundtrip.rs`, at four probes). Two tests need the MDL SDK and are skipped
without it:

- `SHADEWIRE_MDLC=path/to/mdlc` (or `mdlc` on `PATH`) checks every module the writer writes with the SDK's
  compiler.
- `SHADEWIRE_MDL_SDK=path/to/MDL-SDK` parses every `.mdl` file of an SDK checkout and lowers its example
  materials.

## Out of scope

MDL archives (`.mdr`) and MDLE; `texture_3d`, `texture_cube` and `texture_ptex`; light profiles and measured BSDFs;
volumes, backfaces, displacement and hair in either direction; writing named models; OmniPBR and OmniSurface
tuning (they lower from a search path when they evaluate).

Licensed MIT; the vendored MDL SDK files under `mdl/` are BSD-3-Clause (see NOTICE).
