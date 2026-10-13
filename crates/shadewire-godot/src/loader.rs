//! `load("…/x.mtlx")` for documents Godot did not import: `user://`, OS paths, and `res://` files kept as-is in
//! an export. Imported `.mtlx` files load as the imported resource, through Godot's own importer loader.

use godot::classes::{IResourceFormatLoader, ResourceFormatLoader};
use godot::global::Error as GdError;
use godot::prelude::*;

use crate::options::LoadOptions;
use crate::runtime::load_path;

#[derive(GodotClass)]
#[class(init, base=ResourceFormatLoader)]
pub struct ShadewireMtlxLoader {
    base: Base<ResourceFormatLoader>,
}

#[godot_api]
impl IResourceFormatLoader for ShadewireMtlxLoader {
    fn get_recognized_extensions(&self) -> PackedStringArray {
        PackedStringArray::from(&[GString::from("mtlx")])
    }

    fn handles_type(&self, type_: StringName) -> bool {
        ["ShaderMaterial", "Material", "Resource"].contains(&type_.to_string().as_str())
    }

    fn get_resource_type(&self, path: GString) -> GString {
        let mtlx = path.to_string().to_ascii_lowercase().ends_with(".mtlx");
        GString::from(if mtlx { "ShaderMaterial" } else { "" })
    }

    fn load(&self, path: GString, _original_path: GString, _use_sub_threads: bool, _cache_mode: i32) -> Variant {
        let path = path.to_string();
        match load_path(&path, &LoadOptions::default()) {
            Ok(material) => material.to_variant(),
            Err(e) => {
                godot_error!("shadewire: {path}: {e}");
                GdError::ERR_PARSE_ERROR.to_variant()
            }
        }
    }
}
