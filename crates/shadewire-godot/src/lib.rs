//! Shadewire for Godot 4: MaterialX documents as `ShaderMaterial`s, compiled in-process by `shadewire-mtlx` and
//! `shadewire-gdshader`.
//!
//! - Runtime: the `Shadewire` class (`load_material`, `compile_string`, `material_names`) and a resource loader,
//!   so `load("user://x.mtlx")` or `load("D:/assets/x.mtlx")` returns a `ShaderMaterial`.
//! - Editor: an import plugin, so `.mtlx` files in the project import as `ShaderMaterial` resources.
//!
//! Sources are `res://`, `user://` and OS paths; includes and textures resolve against the document's location.

pub mod compile;
pub mod importer;
pub mod io;
pub mod loader;
pub mod location;
pub mod material;
pub mod options;
pub mod runtime;

use std::cell::RefCell;

use godot::classes::ResourceLoader;
use godot::init::InitStage;
use godot::prelude::*;

use crate::loader::ShadewireMtlxLoader;

struct ShadewireExtension;

thread_local! {
    static LOADER: RefCell<Option<Gd<ShadewireMtlxLoader>>> = const { RefCell::new(None) };
}

#[gdextension]
unsafe impl ExtensionLibrary for ShadewireExtension {
    fn on_stage_init(stage: InitStage) {
        if stage == InitStage::Scene {
            let loader = ShadewireMtlxLoader::new_gd();
            ResourceLoader::singleton().add_resource_format_loader(&loader);
            LOADER.set(Some(loader));
        }
    }

    fn on_stage_deinit(stage: InitStage) {
        if stage == InitStage::Scene {
            if let Some(loader) = LOADER.take() {
                ResourceLoader::singleton().remove_resource_format_loader(&loader);
            }
            material::clear_shader_cache();
        }
    }
}
