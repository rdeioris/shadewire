//! The public builder: a MaterialX document to a `ShadewireMaterial` over a base `StandardMaterial`.
//!
//! The builder takes the two asset stores it writes to, `Assets<Image>` (the textures) and
//! `Assets<Shader>` (the generated fragment shader), so it runs wherever a system can borrow them:
//!
//! ```ignore
//! fn spawn(mut images: ResMut<Assets<Image>>, mut shaders: ResMut<Assets<Shader>>, ...) {
//!     let material = ShadewireMaterial::from_mtlx_file("marble.mtlx", None, base, &mut images, &mut shaders)?;
//! }
//! ```
//!
//! Shader cache: the shader is inserted under a `Uuid` derived from the reflection's structure hash and
//! the compile options it does not cover (group, first binding, `flip_v`, `reserve_uniform_binding`),
//! and only when that id is not in `Assets<Shader>` yet. Identical graphs, whatever their uniform
//! values, share one shader and so one pipeline. Shaders inserted by id are never freed.

use std::path::Path;

use bevy::asset::Assets;
use bevy::asset::uuid::Uuid;
use bevy::image::Image;
use bevy::pbr::{ExtendedMaterial, StandardMaterial};
use bevy::shader::Shader;
use shadewire_mtlx::{MtlxOptions, to_ir};
use shadewire_wgsl::{CompileOptions, Compiled, compile};

use crate::error::ShadewireError;
use crate::extension::ShadewireExtension;
use crate::layers;
use crate::plugin::ShadewireMaterial;
use crate::textures;
use crate::uniforms;
use crate::wesl::{compile_options, module};

/// Builds a [`ShadewireMaterial`] from MaterialX. In scope through `use shadewire_bevy::FromMtlx;`.
pub trait FromMtlx: Sized {
    /// Builds the material `material` (the document's first one when `None`) of the MaterialX `text`.
    /// `path` is the document's file: includes resolve against it and texture files against its
    /// directory (the current directory without a path). Textures go to `images`, the generated shader
    /// to `shaders` unless an identical graph put it there already. `base` gets a small clearcoat and
    /// specular transmission when the graph drives them and it has them at 0 (see the README's Bevy
    /// section): Bevy compiles those layers in only for a base that turns them on.
    fn from_mtlx(
        text: &str,
        path: Option<&Path>,
        material: Option<&str>,
        base: StandardMaterial,
        images: &mut Assets<Image>,
        shaders: &mut Assets<Shader>,
    ) -> Result<Self, ShadewireError>;

    /// [`FromMtlx::from_mtlx`] on the contents of the file at `path`.
    fn from_mtlx_file(
        path: impl AsRef<Path>,
        material: Option<&str>,
        base: StandardMaterial,
        images: &mut Assets<Image>,
        shaders: &mut Assets<Shader>,
    ) -> Result<Self, ShadewireError>;
}

impl FromMtlx for ShadewireMaterial {
    fn from_mtlx(
        text: &str,
        path: Option<&Path>,
        material: Option<&str>,
        base: StandardMaterial,
        images: &mut Assets<Image>,
        shaders: &mut Assets<Shader>,
    ) -> Result<Self, ShadewireError> {
        let ir = to_ir(text, path, material, &MtlxOptions::default())?;
        let options = compile_options();
        let compiled = compile(&ir.dag, &ir.source, &options)?;
        let source = module(&compiled)?;
        let params = uniforms::pack(&compiled.reflection)?;
        let dir = path.and_then(Path::parent).unwrap_or(Path::new(""));
        let [t0, t1, t2, t3, t4, t5, t6, t7] = textures::load(&compiled.reflection, dir, images)?;
        let shader = insert_shader(&compiled, &options, source, shaders);
        let extension = ShadewireExtension { params, t0, t1, t2, t3, t4, t5, t6, t7, shader };
        Ok(ExtendedMaterial { base: layers::enable(base, &compiled.reflection), extension })
    }

    fn from_mtlx_file(
        path: impl AsRef<Path>,
        material: Option<&str>,
        base: StandardMaterial,
        images: &mut Assets<Image>,
        shaders: &mut Assets<Shader>,
    ) -> Result<Self, ShadewireError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|source| ShadewireError::Read { path: path.to_path_buf(), source })?;
        Self::from_mtlx(&text, Some(path), material, base, images, shaders)
    }
}

/// The shader's id, inserting `source` under it unless a material with the same id did already.
fn insert_shader(compiled: &Compiled, options: &CompileOptions, source: String, shaders: &mut Assets<Shader>) -> Uuid {
    let id = shader_id(&compiled.reflection.structure_hash, options);
    if !shaders.contains(id) {
        let shader = Shader::from_wesl(source, format!("shadewire/material_{}.wesl", id.simple()));
        shaders.insert(id, shader).expect("a Uuid asset id has no generation to be invalid");
    }
    id
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
pub fn shader_id(structure_hash: &str, options: &CompileOptions) -> Uuid {
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

    fn unlit(value: &str) -> String {
        UNLIT.replace("VALUE", value)
    }

    fn build(text: &str, images: &mut Assets<Image>, shaders: &mut Assets<Shader>) -> ShadewireMaterial {
        ShadewireMaterial::from_mtlx(text, None, None, StandardMaterial::default(), images, shaders).unwrap()
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
        let mut images = Assets::<Image>::default();
        let mut shaders = Assets::<Shader>::default();
        let a = build(&unlit("1, 0.5, 0.25"), &mut images, &mut shaders);
        let b = build(&unlit("1, 0.5, 0.25"), &mut images, &mut shaders);
        let c = build(&unlit("0, 1, 0"), &mut images, &mut shaders);
        assert_eq!(a.extension.shader, b.extension.shader);
        assert_eq!(a.extension.shader, c.extension.shader);
        assert_ne!(a.extension.params, c.extension.params);
        assert_eq!(shaders.len(), 1);
        let shader = shaders.get(a.extension.shader).unwrap();
        assert!(shader.path.starts_with("shadewire/material_"), "{}", shader.path);
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

        let mut images = Assets::<Image>::default();
        let mut shaders = Assets::<Shader>::default();
        let base = StandardMaterial::default();
        let m = ShadewireMaterial::from_mtlx_file(dir.join("m.mtlx"), None, base.clone(), &mut images, &mut shaders)
            .unwrap();
        assert!(m.extension.t0.is_some() && m.extension.t1.is_none());
        assert_eq!(images.len(), 1);

        let err = ShadewireMaterial::from_mtlx_file(dir.join("missing.mtlx"), None, base, &mut images, &mut shaders)
            .unwrap_err();
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
        let mut images = Assets::<Image>::default();
        let mut shaders = Assets::<Shader>::default();
        let m = build(text, &mut images, &mut shaders);
        assert!(m.base.clearcoat > 0.0);
        assert_eq!(m.base.specular_transmission, 0.0);
    }

    #[test]
    fn a_missing_mtlx_file_is_named() {
        let mut images = Assets::<Image>::default();
        let mut shaders = Assets::<Shader>::default();
        let err =
            ShadewireMaterial::from_mtlx_file("no/such.mtlx", None, default(), &mut images, &mut shaders).unwrap_err();
        assert!(err.to_string().contains("such.mtlx"), "{err}");
    }

    #[test]
    fn vendored_examples_build_in_a_headless_app() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), ImagePlugin::default()));
        app.init_asset::<Shader>();
        let built = app
            .world_mut()
            .run_system_once(|mut images: ResMut<Assets<Image>>, mut shaders: ResMut<Assets<Shader>>| {
                let mut ids = Vec::new();
                for name in ["standard_surface_default", "standard_surface_marble_solid", "standard_surface_default"] {
                    let file = format!("{EXAMPLES}/StandardSurface/{name}.mtlx");
                    let base = StandardMaterial::default();
                    let m = ShadewireMaterial::from_mtlx_file(&file, None, base, &mut images, &mut shaders).unwrap();
                    ids.push(m.extension.shader);
                }
                (ids, shaders.len())
            })
            .unwrap();
        let (ids, count) = built;
        assert_eq!(ids[0], ids[2]);
        assert_ne!(ids[0], ids[1]);
        assert_eq!(count, 2);
    }
}
