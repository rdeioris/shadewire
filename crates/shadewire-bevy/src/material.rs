//! The public builder: a MaterialX document to a `ShadewireMaterial` over a base `StandardMaterial`, with
//! the reflection that names its uniforms.
//!
//! The builder writes textures to `Assets<Image>` and the generated fragment shader to `Assets<Shader>`,
//! both borrowed through the [`ShadewireAssets`] system parameter:
//!
//! ```ignore
//! fn spawn(mut assets: ShadewireAssets, ...) {
//!     let built = ShadewireMaterial::from_mtlx_file("marble.mtlx", &BuildOptions::default(), base, &mut assets)?;
//! }
//! ```
//!
//! Shader cache: the shader is inserted under a `Uuid` derived from the reflection's structure hash and
//! the compile options it does not cover (group, first binding, `flip_v`, `reserve_uniform_binding`),
//! and only when that id is not in `Assets<Shader>` yet. Identical graphs, whatever their uniform
//! values, share one shader and so one pipeline. The shader is freed once no material holds it
//! (see `cache`).

use std::path::Path;

use bevy::asset::Assets;
use bevy::asset::uuid::Uuid;
use bevy::image::Image;
use bevy::pbr::{ExtendedMaterial, StandardMaterial};
use bevy::shader::Shader;
use shadewire_mtlx::to_ir;
use shadewire_wgsl::{CompileOptions, compile};

use crate::assets::ShadewireAssets;
use crate::built::Built;
use crate::cache::ShaderCache;
use crate::error::ShadewireError;
use crate::extension::ShadewireExtension;
use crate::layers;
use crate::lobes;
use crate::options::BuildOptions;
use crate::plugin::ShadewireMaterial;
use crate::shader::module;
use crate::textures;
use crate::uniforms;

/// Builds a [`ShadewireMaterial`] from MaterialX. In scope through `use shadewire_bevy::FromMtlx;`.
pub trait FromMtlx: Sized {
    /// Builds `options.material` (needed only when the document has several) of the MaterialX `text`, with
    /// its reflection. `path` is the document's file: includes resolve against it and texture files
    /// against its directory (the current directory without a path). `base` gets a small clearcoat and
    /// specular transmission when the graph drives them and it has them at 0 (see the README's Bevy
    /// section): Bevy compiles those layers in only for a base that turns them on.
    fn from_mtlx(
        text: &str,
        path: Option<&Path>,
        options: &BuildOptions,
        base: StandardMaterial,
        assets: &mut ShadewireAssets,
    ) -> Result<Built<Self>, ShadewireError>;

    /// [`FromMtlx::from_mtlx`] on the contents of the file at `path`.
    fn from_mtlx_file(
        path: impl AsRef<Path>,
        options: &BuildOptions,
        base: StandardMaterial,
        assets: &mut ShadewireAssets,
    ) -> Result<Built<Self>, ShadewireError>;
}

impl FromMtlx for ShadewireMaterial {
    fn from_mtlx(
        text: &str,
        path: Option<&Path>,
        options: &BuildOptions,
        base: StandardMaterial,
        assets: &mut ShadewireAssets,
    ) -> Result<Built<Self>, ShadewireError> {
        build(text, path, options, base, &mut Stores::of(assets))
    }

    fn from_mtlx_file(
        path: impl AsRef<Path>,
        options: &BuildOptions,
        base: StandardMaterial,
        assets: &mut ShadewireAssets,
    ) -> Result<Built<Self>, ShadewireError> {
        build_file(path.as_ref(), options, base, &mut Stores::of(assets))
    }
}

/// The builder's stores as plain borrows, so tests can build without an app.
pub(crate) struct Stores<'a> {
    pub images: &'a mut Assets<Image>,
    pub shaders: &'a mut Assets<Shader>,
    pub cache: &'a mut ShaderCache,
}

impl<'a> Stores<'a> {
    fn of(assets: &'a mut ShadewireAssets) -> Stores<'a> {
        Stores { images: &mut assets.images, shaders: &mut assets.shaders, cache: &mut assets.cache }
    }
}

pub(crate) fn build(
    text: &str,
    path: Option<&Path>,
    options: &BuildOptions,
    base: StandardMaterial,
    stores: &mut Stores,
) -> Result<Built<ShadewireMaterial>, ShadewireError> {
    let location = path.map(|p| p.display().to_string());
    let ir = to_ir(text, location.as_deref(), options.material.as_deref(), &options.ir, &options.resolver)?;
    let compile_options = options.compile_options();
    let mut compiled = compile(&ir.dag, &ir.source, &compile_options)?;
    let source = module(&compiled)?;
    if compiled.reflection.closure.is_some() {
        compiled.reflection.approximations.push(lobes::APPROXIMATIONS.to_string());
    }
    let params = uniforms::pack(&compiled.reflection)?;
    let dir = path.and_then(Path::parent).unwrap_or(Path::new(""));
    let [t0, t1, t2, t3, t4, t5, t6, t7] = textures::load(&compiled.reflection, dir, stores.images)?;
    let shader = shader_id(&compiled.reflection.structure_hash, &compile_options);
    let hold = stores.cache.hold(shader, stores.shaders, || {
        Shader::from_wesl(source, format!("shadewire/material_{}.wesl", shader.simple()))
    });
    let extension = ShadewireExtension { params, t0, t1, t2, t3, t4, t5, t6, t7, shader, hold };
    let material = ExtendedMaterial { base: layers::enable(base, &compiled.reflection), extension };
    Ok(Built { material, reflection: compiled.reflection })
}

pub(crate) fn build_file(
    path: &Path,
    options: &BuildOptions,
    base: StandardMaterial,
    stores: &mut Stores,
) -> Result<Built<ShadewireMaterial>, ShadewireError> {
    let text =
        std::fs::read_to_string(path).map_err(|source| ShadewireError::Read { path: path.to_path_buf(), source })?;
    build(&text, Some(path), options, base, stores)
}

/// 64-bit FNV-1a.
fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// The shader id of a structure hash under `options`: the high half is the structure hash (hex, as
/// the reflection writes it), the low half a hash of the options that change the generated WGSL.
pub(crate) fn shader_id(structure_hash: &str, options: &CompileOptions) -> Uuid {
    let high = u64::from_str_radix(structure_hash, 16).unwrap_or_else(|_| fnv(structure_hash.as_bytes()));
    let mut key = b"shadewire-bevy shader".to_vec();
    key.extend_from_slice(&options.group.to_le_bytes());
    key.extend_from_slice(&options.first_binding.to_le_bytes());
    key.push(options.flip_v as u8);
    key.push(options.reserve_uniform_binding as u8);
    Uuid::from_u64_pair(high, fnv(&key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::AssetPlugin;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::image::ImagePlugin;
    use bevy::prelude::*;
    use shadewire::Value;
    use shadewire_nodes::IrOptions;

    use crate::ShadewirePlugin;
    use crate::shader::compile_options;

    const EXAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../vendor/materialx/resources/Materials/Examples");

    const UNLIT: &str = r#"<materialx version="1.39">
  <surface_unlit name="SU" type="surfaceshader">
    <input name="emission_color" type="color3" value="VALUE" />
  </surface_unlit>
  <surfacematerial name="M" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="SU" />
  </surfacematerial>
</materialx>"#;

    const TEXTURED: &str = r#"<materialx version="1.39" fileprefix="tex/">
  <image name="img" type="color3">
    <input name="file" type="filename" value="FILE" />
  </image>
  <surface_unlit name="SU" type="surfaceshader">
    <input name="emission_color" type="color3" nodename="img" />
  </surface_unlit>
  <surfacematerial name="M" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="SU" />
  </surfacematerial>
</materialx>"#;

    const TWO: &str = r#"<materialx version="1.39">
  <surface_unlit name="A" type="surfaceshader">
    <input name="emission_color" type="color3" value="1, 0, 0" />
  </surface_unlit>
  <surface_unlit name="B" type="surfaceshader">
    <input name="emission_color" type="color3" value="0, 0, 1" />
    <input name="emission" type="float" value="0.5" />
  </surface_unlit>
  <surfacematerial name="MA" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="A" />
  </surfacematerial>
  <surfacematerial name="MB" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="B" />
  </surfacematerial>
</materialx>"#;

    fn unlit(value: &str) -> String {
        UNLIT.replace("VALUE", value)
    }

    #[derive(Default)]
    struct Fixture {
        images: Assets<Image>,
        shaders: Assets<Shader>,
        cache: ShaderCache,
    }

    impl Fixture {
        fn stores(&mut self) -> Stores<'_> {
            Stores { images: &mut self.images, shaders: &mut self.shaders, cache: &mut self.cache }
        }

        fn build_with(&mut self, text: &str, options: &BuildOptions) -> Built<ShadewireMaterial> {
            build(text, None, options, StandardMaterial::default(), &mut self.stores()).unwrap()
        }

        fn build(&mut self, text: &str) -> ShadewireMaterial {
            self.build_with(text, &BuildOptions::default()).material
        }

        fn build_file(&mut self, path: impl AsRef<Path>) -> Result<Built<ShadewireMaterial>, ShadewireError> {
            build_file(path.as_ref(), &BuildOptions::default(), StandardMaterial::default(), &mut self.stores())
        }
    }

    #[test]
    fn shader_id_is_deterministic_and_keyed_on_every_option() {
        let o = compile_options();
        let id = shader_id("0123456789abcdef", &o);
        assert_eq!(id, shader_id("0123456789abcdef", &o));
        assert_eq!(id.as_u64_pair().0, 0x0123_4567_89ab_cdef);
        assert_ne!(id, shader_id("0123456789abcdee", &o));
        assert_ne!(id, shader_id("0123456789abcdef", &CompileOptions { group: 2, ..o.clone() }));
        assert_ne!(id, shader_id("0123456789abcdef", &CompileOptions { first_binding: 0, ..o.clone() }));
        assert_ne!(id, shader_id("0123456789abcdef", &CompileOptions { flip_v: false, ..o.clone() }));
        assert_ne!(id, shader_id("0123456789abcdef", &CompileOptions { reserve_uniform_binding: false, ..o.clone() }));
    }

    #[test]
    fn identical_graphs_share_one_shader() {
        let mut f = Fixture::default();
        let a = f.build(&unlit("1, 0.5, 0.25"));
        let b = f.build(&unlit("1, 0.5, 0.25"));
        let c = f.build(&unlit("0, 1, 0"));
        assert_eq!(a.extension.shader, b.extension.shader);
        assert_eq!(a.extension.shader, c.extension.shader);
        assert_eq!(a.extension.hold.id(), Some(a.extension.shader));
        assert_ne!(a.extension.params, c.extension.params);
        assert_eq!(f.shaders.len(), 1);
        let shader = f.shaders.get(a.extension.shader).unwrap();
        assert!(shader.path.starts_with("shadewire/material_"), "{}", shader.path);
    }

    #[test]
    fn the_shader_is_freed_with_its_last_material() {
        let mut f = Fixture::default();
        let a = f.build(&unlit("1, 0.5, 0.25"));
        let b = a.clone();
        let c = f.build(&unlit("0, 1, 0"));
        let id = a.extension.shader;
        drop(a);
        drop(c);
        assert_eq!(f.cache.free_unused(&mut f.shaders), 0);
        assert!(f.shaders.contains(id));
        drop(b);
        assert_eq!(f.cache.free_unused(&mut f.shaders), 1);
        assert!(f.shaders.is_empty() && f.cache.is_empty());
        let again = f.build(&unlit("1, 1, 1"));
        assert_eq!(again.extension.shader, id);
        assert!(f.shaders.contains(id));
    }

    #[test]
    fn ir_options_pass_through() {
        let mut f = Fixture::default();
        let uniforms = f.build_with(&unlit("1, 0.5, 0.25"), &BuildOptions::default());
        let options = BuildOptions {
            ir: IrOptions { values_as_uniforms: false, ..IrOptions::default() },
            ..BuildOptions::default()
        };
        let constants = f.build_with(&unlit("1, 0.5, 0.25"), &options);
        assert!(!uniforms.reflection.uniforms.is_empty());
        assert!(constants.reflection.uniforms.is_empty());
        assert_eq!(constants.reflection.params["emission_color"].kind, "constant");
        assert_ne!(uniforms.material.extension.shader, constants.material.extension.shader);
        assert_eq!(f.shaders.len(), 2);

        let refused = BuildOptions { ir: IrOptions::with_models(&["standard_surface"]), ..BuildOptions::default() };
        assert!(build(&unlit("1, 1, 1"), None, &refused, default(), &mut f.stores()).is_err());
    }

    #[test]
    fn flip_v_is_a_different_shader() {
        let mut f = Fixture::default();
        let flipped = f.build_with(&unlit("1, 1, 1"), &BuildOptions::default());
        let unflipped = f.build_with(&unlit("1, 1, 1"), &BuildOptions { flip_v: false, ..BuildOptions::default() });
        assert_ne!(flipped.material.extension.shader, unflipped.material.extension.shader);
    }

    #[test]
    fn the_named_material_is_built() {
        let mut f = Fixture::default();
        let a = f.build_with(TWO, &BuildOptions::material("MA"));
        let b = f.build_with(TWO, &BuildOptions::material("MB"));
        assert_ne!(a.material.extension.params, b.material.extension.params);
        assert_ne!(a.reflection.uniforms.len(), b.reflection.uniforms.len());
        let err = build(TWO, None, &BuildOptions::default(), default(), &mut f.stores()).unwrap_err();
        assert!(err.to_string().contains("MA"), "{err}");
    }

    #[test]
    fn uniforms_are_set_by_name_through_the_reflection() {
        let mut f = Fixture::default();
        let Built { mut material, reflection } = f.build_with(&unlit("1, 0.5, 0.25"), &BuildOptions::default());
        let name = reflection.params["emission_color"].uniform.clone().expect("a uniform field");
        let expected = f.build(&unlit("0, 1, 0")).extension.params;
        assert_ne!(material.extension.params, expected);
        material.extension.set_uniform(&reflection, &name, &Value::Color3([0.0, 1.0, 0.0])).unwrap();
        assert_eq!(material.extension.params, expected);
        let err = material.extension.set_uniform(&reflection, "nope", &Value::Float(1.0)).unwrap_err();
        assert!(matches!(err, ShadewireError::UnknownUniform(_)), "{err}");
    }

    #[test]
    fn textures_load_relative_to_the_mtlx_file() {
        let dir = std::env::temp_dir().join(format!("shadewire-bevy-material-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("tex")).unwrap();
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
        encoder.set_color(png::ColorType::Rgb);
        encoder.write_header().unwrap().write_image_data(&[0u8; 12]).unwrap();
        std::fs::write(dir.join("tex/grid.png"), bytes).unwrap();
        std::fs::write(dir.join("m.mtlx"), TEXTURED.replace("FILE", "grid.png")).unwrap();
        std::fs::write(dir.join("missing.mtlx"), TEXTURED.replace("FILE", "nowhere.png")).unwrap();

        let mut f = Fixture::default();
        let m = f.build_file(dir.join("m.mtlx")).unwrap().material;
        assert!(m.extension.t0.is_some() && m.extension.t1.is_none());
        assert_eq!(f.images.len(), 1);

        let err = f.build_file(dir.join("missing.mtlx")).unwrap_err();
        assert!(err.to_string().contains("nowhere.png"), "{err}");
    }

    #[test]
    fn a_coated_graph_turns_clearcoat_on_in_the_base() {
        let text = r#"<materialx version="1.39">
  <standard_surface name="S" type="surfaceshader">
    <input name="coat" type="float" value="1" />
  </standard_surface>
  <surfacematerial name="M" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="S" />
  </surfacematerial>
</materialx>"#;
        let m = Fixture::default().build(text);
        assert!(m.base.clearcoat > 0.0);
        assert_eq!(m.base.specular_transmission, 0.0);
    }

    #[test]
    fn a_closure_tree_is_built_and_says_what_it_is_not_lit_by() {
        let options = BuildOptions::lobes();
        let mut f = Fixture::default();
        let text =
            std::fs::read_to_string(format!("{EXAMPLES}/StandardSurface/standard_surface_default.mtlx")).unwrap();
        let built = f.build_with(&text, &options);
        assert_eq!(built.reflection.model, "surface");
        assert!(built.reflection.approximations.iter().any(|a| a == lobes::APPROXIMATIONS));
        let shader = f.shaders.get(built.material.extension.shader).unwrap();
        assert!(shader.path.starts_with("shadewire/material_"), "{}", shader.path);
        let mapped = f.build_with(&text, &BuildOptions::default());
        assert!(mapped.reflection.approximations.iter().all(|a| a != lobes::APPROXIMATIONS));
    }

    #[test]
    fn a_missing_mtlx_file_is_named() {
        let err = Fixture::default().build_file("no/such.mtlx").unwrap_err();
        assert!(err.to_string().contains("such.mtlx"), "{err}");
    }

    fn headless() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), ImagePlugin::default()));
        app.init_asset::<Shader>();
        app.add_plugins(ShadewirePlugin);
        app
    }

    #[test]
    fn vendored_examples_build_in_a_headless_app() {
        let mut app = headless();
        let built = app
            .world_mut()
            .run_system_once(|mut assets: ShadewireAssets| {
                let mut ids = Vec::new();
                for name in ["standard_surface_default", "standard_surface_marble_solid", "standard_surface_default"] {
                    let file = format!("{EXAMPLES}/StandardSurface/{name}.mtlx");
                    let options = BuildOptions::default();
                    let m = ShadewireMaterial::from_mtlx_file(&file, &options, default(), &mut assets).unwrap();
                    ids.push(m.material.extension.shader);
                }
                (ids, assets.shaders.len())
            })
            .unwrap();
        let (ids, count) = built;
        assert_eq!(ids[0], ids[2]);
        assert_ne!(ids[0], ids[1]);
        assert_eq!(count, 2);
    }

    #[test]
    fn the_plugin_frees_a_removed_materials_shader() {
        let mut app = headless();
        let text = unlit("1, 0.5, 0.25");
        let (handle, id) = app
            .world_mut()
            .run_system_once(move |mut assets: ShadewireAssets, mut materials: ResMut<Assets<ShadewireMaterial>>| {
                let options = BuildOptions::default();
                let built = ShadewireMaterial::from_mtlx(&text, None, &options, default(), &mut assets).unwrap();
                let id = built.material.extension.shader;
                (materials.add(built.material), id)
            })
            .unwrap();
        app.update();
        app.update();
        assert!(app.world().resource::<Assets<Shader>>().contains(id));
        drop(handle);
        app.update();
        app.update();
        assert!(!app.world().resource::<Assets<Shader>>().contains(id));
        assert!(app.world().resource::<ShaderCache>().is_empty());
    }
}
