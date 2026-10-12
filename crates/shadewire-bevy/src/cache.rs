//! Generated shaders, freed when no material holds them.
//!
//! A shader is inserted into `Assets<Shader>` under a `Uuid` (the pipeline key must be `Copy`), and Bevy
//! does not ref-count a `Uuid` asset. So each material carries a [`ShaderHold`], a shared count on its
//! shader's id that clones of the material share. [`free_unused_shaders`], which `ShadewirePlugin` runs
//! in `Last`, removes the shaders whose holds are all dropped. A material built but not yet added to
//! `Assets<ShadewireMaterial>` holds its shader too.

use std::sync::{Arc, Weak};

use bevy::asset::uuid::Uuid;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::shader::Shader;

/// A material's share of its generated shader. `Default` holds nothing: a material built by hand with it
/// keeps no shader alive.
#[derive(Clone, Debug, Default)]
pub struct ShaderHold(Option<Arc<Uuid>>);

impl ShaderHold {
    /// The id of the held shader.
    pub fn id(&self) -> Option<Uuid> {
        self.0.as_deref().copied()
    }
}

/// The shaders the builder inserted, each with the holds of the materials using it.
#[derive(Resource, Default, Debug)]
pub struct ShaderCache {
    held: HashMap<Uuid, Weak<Uuid>>,
}

impl ShaderCache {
    /// A hold on the shader `id`, inserting `make()` under it unless `shaders` has it.
    pub(crate) fn hold(&mut self, id: Uuid, shaders: &mut Assets<Shader>, make: impl FnOnce() -> Shader) -> ShaderHold {
        if !shaders.contains(id) {
            shaders.insert(id, make()).expect("a Uuid asset id has no generation to be invalid");
        }
        let live = self.held.get(&id).and_then(Weak::upgrade);
        let arc = live.unwrap_or_else(|| {
            let arc = Arc::new(id);
            self.held.insert(id, Arc::downgrade(&arc));
            arc
        });
        ShaderHold(Some(arc))
    }

    /// Cached shaders, held or not yet freed.
    pub fn len(&self) -> usize {
        self.held.len()
    }

    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    /// Whether some material holds the shader `id`.
    pub fn is_held(&self, id: Uuid) -> bool {
        self.held.get(&id).is_some_and(|w| w.strong_count() > 0)
    }

    fn any_unheld(&self) -> bool {
        self.held.values().any(|w| w.strong_count() == 0)
    }

    /// Removes from `shaders` every cached shader no material holds; the number removed.
    pub fn free_unused(&mut self, shaders: &mut Assets<Shader>) -> usize {
        let before = self.held.len();
        self.held.retain(|id, weak| {
            let held = weak.strong_count() > 0;
            if !held {
                shaders.remove(*id);
            }
            held
        });
        before - self.held.len()
    }
}

/// [`ShaderCache::free_unused`] once a frame, touching neither resource while every shader is held.
pub fn free_unused_shaders(mut cache: ResMut<ShaderCache>, mut shaders: ResMut<Assets<Shader>>) {
    if cache.any_unheld() {
        cache.free_unused(&mut shaders);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shader() -> Shader {
        Shader::from_wgsl("", "t.wgsl")
    }

    #[test]
    fn a_shader_lives_while_one_hold_does() {
        let mut cache = ShaderCache::default();
        let mut shaders = Assets::<Shader>::default();
        let id = Uuid::from_u128(7);
        let a = cache.hold(id, &mut shaders, shader);
        let b = cache.hold(id, &mut shaders, || unreachable!("inserted once"));
        let c = a.clone();
        assert_eq!((a.id(), cache.len(), shaders.len()), (Some(id), 1, 1));
        drop(a);
        drop(b);
        assert_eq!(cache.free_unused(&mut shaders), 0);
        assert!(cache.is_held(id) && shaders.contains(id));
        drop(c);
        assert!(!cache.is_held(id));
        assert_eq!(cache.free_unused(&mut shaders), 1);
        assert!(cache.is_empty() && !shaders.contains(id));
    }

    #[test]
    fn a_hold_after_the_last_drop_keeps_the_shader() {
        let mut cache = ShaderCache::default();
        let mut shaders = Assets::<Shader>::default();
        let id = Uuid::from_u128(8);
        drop(cache.hold(id, &mut shaders, shader));
        let again = cache.hold(id, &mut shaders, || unreachable!("still there"));
        assert_eq!(cache.free_unused(&mut shaders), 0);
        assert!(shaders.contains(id));
        drop(again);
        assert_eq!(cache.free_unused(&mut shaders), 1);
    }

    #[test]
    fn a_freed_shader_is_inserted_again() {
        let mut cache = ShaderCache::default();
        let mut shaders = Assets::<Shader>::default();
        let id = Uuid::from_u128(9);
        drop(cache.hold(id, &mut shaders, shader));
        cache.free_unused(&mut shaders);
        let _hold = cache.hold(id, &mut shaders, shader);
        assert!(shaders.contains(id) && cache.is_held(id));
    }
}
