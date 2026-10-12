//! The MaterialX materials in use, each built over a base `StandardMaterial`, and `R` to rebuild them
//! all from the `.mtlx` file.

use std::path::PathBuf;

use bevy::ecs::system::SystemParam;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use shadewire_bevy::{FromMtlx, ShadewireError, ShadewireMaterial};

/// The `.mtlx` file and the material to build from it.
#[derive(Resource, Clone)]
pub struct Source {
    pub mtlx: PathBuf,
    pub material: Option<String>,
}

impl Source {
    /// Builds the material over `base`.
    pub fn build(
        &self,
        base: StandardMaterial,
        images: &mut Assets<Image>,
        shaders: &mut Assets<Shader>,
    ) -> Result<ShadewireMaterial, ShadewireError> {
        ShadewireMaterial::from_mtlx_file(&self.mtlx, self.material.as_deref(), base, images, shaders)
    }
}

/// Every Shadewire material made so far with the base it was built on. One per base: meshes that
/// shared a `StandardMaterial` share the Shadewire material that replaced it.
#[derive(Resource, Default)]
pub struct Swaps {
    pub entries: Vec<(StandardMaterial, Handle<ShadewireMaterial>)>,
    pub by_base: HashMap<AssetId<StandardMaterial>, Handle<ShadewireMaterial>>,
}

/// What building a material needs: the source, the asset stores it writes and the record of swaps.
#[derive(SystemParam)]
pub struct Builder<'w> {
    source: Res<'w, Source>,
    images: ResMut<'w, Assets<Image>>,
    shaders: ResMut<'w, Assets<Shader>>,
    materials: ResMut<'w, Assets<ShadewireMaterial>>,
    swaps: ResMut<'w, Swaps>,
}

impl Builder<'_> {
    /// The Shadewire material over `base`: the one made earlier for the asset `id`, else a new one.
    /// A build error is printed and returned as `None`.
    pub fn material(
        &mut self,
        id: Option<AssetId<StandardMaterial>>,
        base: &StandardMaterial,
    ) -> Option<Handle<ShadewireMaterial>> {
        if let Some(handle) = id.and_then(|id| self.swaps.by_base.get(&id)) {
            return Some(handle.clone());
        }
        let material = match self.source.build(base.clone(), &mut self.images, &mut self.shaders) {
            Ok(material) => material,
            Err(err) => {
                eprintln!("error: {}: {err}", self.source.mtlx.display());
                return None;
            }
        };
        let handle = self.materials.add(material);
        self.swaps.entries.push((base.clone(), handle.clone()));
        if let Some(id) = id {
            self.swaps.by_base.insert(id, handle.clone());
        }
        Some(handle)
    }

    /// Recompiles the `.mtlx` for every base and replaces the materials in place. A failure is printed
    /// and leaves every material as it was.
    pub fn reload(&mut self) {
        let mut rebuilt = Vec::new();
        for (base, _) in &self.swaps.entries {
            match self.source.build(base.clone(), &mut self.images, &mut self.shaders) {
                Ok(material) => rebuilt.push(material),
                Err(err) => {
                    eprintln!("error: {}: {err}", self.source.mtlx.display());
                    return;
                }
            }
        }
        for ((_, handle), material) in self.swaps.entries.iter().zip(rebuilt) {
            self.materials.insert(handle, material).expect("the handle is alive");
        }
        println!("reloaded {} ({} materials)", self.source.mtlx.display(), self.swaps.entries.len());
    }
}

/// `R`: [`Builder::reload`].
pub fn reload(keys: Res<ButtonInput<KeyCode>>, mut builder: Builder) {
    if keys.just_pressed(KeyCode::KeyR) {
        builder.reload();
    }
}
