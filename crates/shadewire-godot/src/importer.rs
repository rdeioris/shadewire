//! The editor side: `.mtlx` files in the project import as `ShaderMaterial`s, with the same compile core as the
//! runtime. Their textures are ordinary project files, imported by Godot first.

use std::sync::Arc;

use godot::classes::file_access::ModeFlags;
use godot::classes::{
    ConfigFile, EditorImportPlugin, EditorPlugin, FileAccess, IEditorImportPlugin, IEditorPlugin, ResourceSaver,
    ShaderMaterial,
};
use godot::global::Error as GdError;
use godot::prelude::*;
use godot::register::info::PropertyHint;
use shadewire_mtlx::material_names;

use crate::compile::{compile, reference};
use crate::io::{load_texture, read_text, resolver};
use crate::material::build;
use crate::options::{Budget, Limits, LoadOptions, TRANSPARENCY};
use crate::runtime::options_from;

#[derive(GodotClass)]
#[class(tool, init, base=EditorImportPlugin)]
pub struct ShadewireMtlxImporter {
    base: Base<EditorImportPlugin>,
}

fn option(name: &str, default: Variant, hint: PropertyHint, hint_string: &str) -> AnyDictionary {
    vdict! {
        "name" => name,
        "default_value" => &default,
        "property_hint" => hint.ord(),
        "hint_string" => hint_string,
    }
    .upcast_any_dictionary()
}

fn import(
    source: &str,
    save_path: &str,
    options: &VarDictionary,
    gen_files: &mut Array<GString>,
) -> Result<(), String> {
    let mut options = options.duplicate_shallow();
    let write_gdshader = options.remove("write_gdshader").is_some_and(|v| v.try_to::<bool>().unwrap_or(false));
    // The project's own files: no limits for untrusted documents.
    let options = LoadOptions { limits: Limits::NONE, ..options_from(&options)? };
    let budget = Arc::new(Budget::default());
    let text = read_text(source, &budget)?;
    // A document of definitions or node graphs only (an include): an empty material, so the import succeeds.
    if material_names(&text, Some(source), &resolver(budget.clone())).map_err(|e| e.to_string())?.is_empty() {
        return save(&ShaderMaterial::new_gd(), save_path);
    }
    let out = compile(&text, Some(source), &options, resolver(budget.clone()))?;
    let built = build(&out, Some(source), options.strict_textures, false, |path| load_texture(path, &budget, None))?;
    let without_mipmaps = out.reflection.textures.iter().filter_map(|t| reference(Some(source), &t.file).ok());
    let without_mipmaps = without_mipmaps.filter(|path| !imports_mipmaps(path)).map(|path| {
        format!(
            "texture {path} is imported without mipmaps (it aliases): enable Mipmaps > Generate in its import options"
        )
    });
    for warning in built.warnings.iter().cloned().chain(without_mipmaps) {
        godot_warn!("shadewire: {source}: {warning}");
    }
    if write_gdshader {
        let path = format!("{}.gdshader", GString::from(source).get_basename());
        let mut file = FileAccess::open(&path, ModeFlags::WRITE).ok_or_else(|| format!("cannot write {path}"))?;
        file.store_string(&out.code);
        file.close();
        gen_files.push(&path);
    }
    save(&built.material, save_path)
}

/// Whether Godot's import of the image at `path` generates mipmaps; true when there is no import to ask.
fn imports_mipmaps(path: &str) -> bool {
    let mut config = ConfigFile::new_gd();
    config.load(&format!("{path}.import")) != GdError::OK
        || config.get_value_ex("params", "mipmaps/generate").default(&true.to_variant()).done().booleanize()
}

fn save(material: &Gd<ShaderMaterial>, save_path: &str) -> Result<(), String> {
    let status = ResourceSaver::singleton().save_ex(material).path(&format!("{save_path}.res")).done();
    if status != GdError::OK {
        return Err(format!("cannot save {save_path}.res: {status:?}"));
    }
    Ok(())
}

#[godot_api]
impl IEditorImportPlugin for ShadewireMtlxImporter {
    fn get_importer_name(&self) -> GString {
        "shadewire.mtlx".into()
    }

    fn get_visible_name(&self) -> GString {
        "MaterialX (Shadewire)".into()
    }

    fn get_recognized_extensions(&self) -> PackedStringArray {
        PackedStringArray::from(&[GString::from("mtlx")])
    }

    fn get_save_extension(&self) -> GString {
        "res".into()
    }

    fn get_resource_type(&self) -> GString {
        "ShaderMaterial".into()
    }

    fn get_preset_count(&self) -> i32 {
        1
    }

    fn get_preset_name(&self, _preset_index: i32) -> GString {
        "Default".into()
    }

    fn get_priority(&self) -> f32 {
        1.0
    }

    /// After textures (order 0), so the document's images are imported resources by the time it loads them.
    fn get_import_order(&self) -> i32 {
        100
    }

    fn get_format_version(&self) -> i32 {
        1
    }

    /// Not threaded: loading the document's textures from an import worker thread deadlocks Godot 4.7.2's
    /// headless import.
    fn can_import_threaded(&self) -> bool {
        false
    }

    fn get_option_visibility(&self, _path: GString, _option_name: StringName, _options: VarDictionary) -> bool {
        true
    }

    fn get_import_options(&self, path: GString, _preset_index: i32) -> Array<AnyDictionary> {
        let path = path.to_string();
        let budget = Arc::new(Budget::default());
        let names = read_text(&path, &budget)
            .and_then(|text| material_names(&text, Some(&path), &resolver(budget.clone())).map_err(|e| e.to_string()))
            .unwrap_or_default();
        let first = names.first().cloned().unwrap_or_default();
        let defaults = LoadOptions::default();
        let transparency: Vec<&str> = TRANSPARENCY.iter().map(|(name, _)| *name).collect();
        let mut options = Array::new();
        options.push(option("material", first.to_variant(), PropertyHint::ENUM, &names.join(",")));
        options.push(option("models", "mapped".to_variant(), PropertyHint::ENUM, "mapped,lobes"));
        options.push(option("bake_constants", defaults.bake_constants.to_variant(), PropertyHint::NONE, ""));
        options.push(option("optimize", defaults.optimize.to_variant(), PropertyHint::NONE, ""));
        options.push(option("transparency", "auto".to_variant(), PropertyHint::ENUM, &transparency.join(",")));
        options.push(option(
            "emission_scale",
            defaults.emission_scale.to_variant(),
            PropertyHint::RANGE,
            "0,100,0.01,or_greater",
        ));
        options.push(option("strict_textures", defaults.strict_textures.to_variant(), PropertyHint::NONE, ""));
        options.push(option("write_gdshader", false.to_variant(), PropertyHint::NONE, ""));
        options
    }

    fn import(
        &mut self,
        source_file: GString,
        save_path: GString,
        options: VarDictionary,
        _platform_variants: Array<GString>,
        mut gen_files: Array<GString>,
    ) -> GdError {
        let source = source_file.to_string();
        match import(&source, &save_path.to_string(), &options, &mut gen_files) {
            Ok(()) => GdError::OK,
            Err(e) => {
                godot_error!("shadewire: {source}: {e}");
                GdError::ERR_PARSE_ERROR
            }
        }
    }
}

/// Registers the importer in the editor. gdext editor plugins are always on: they do not appear under Project
/// Settings > Plugins.
#[derive(GodotClass)]
#[class(tool, init, base=EditorPlugin)]
pub struct ShadewirePlugin {
    base: Base<EditorPlugin>,
    importer: Option<Gd<ShadewireMtlxImporter>>,
}

#[godot_api]
impl IEditorPlugin for ShadewirePlugin {
    fn enter_tree(&mut self) {
        let importer = ShadewireMtlxImporter::new_gd();
        self.base_mut().add_import_plugin(&importer);
        self.importer = Some(importer);
    }

    fn exit_tree(&mut self) {
        if let Some(importer) = self.importer.take() {
            self.base_mut().remove_import_plugin(&importer);
        }
    }
}
