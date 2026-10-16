//! The NVIDIA MDL front end (Python's `shadewire.mdl`): `shadewire_mdl_to_ir`, `shadewire_mdl_material_names`. The
//! MDL back end is `shadewire_compile` with `SHADEWIRE_TARGET_MDL`.

use std::os::raw::c_char;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;

use shadewire_mdl::Resolver;

use crate::error::{CapiError, shadewire_error};
use crate::guard::guarded;
use crate::ir::shadewire_ir;
use crate::names::shadewire_strings;
use crate::options::{read_ir_options, shadewire_ir_options};
use crate::status::shadewire_status;
use crate::strings::{ArgError, opt_cstr, string_list, text_arg};

/// The resolver searching the `path_count` directories at `paths` for imported modules.
///
/// # Safety
/// As `string_list`.
unsafe fn resolver(paths: *const *const c_char, path_count: usize) -> Result<Resolver, ArgError> {
    let paths = unsafe { string_list(paths, path_count, "path") }?;
    Ok(Resolver::new(paths.into_iter().map(PathBuf::from).collect()))
}

/// Lowers material `material` of the MDL module `text` (`len` bytes of UTF-8, not NUL-terminated) to the IR and writes
/// the handle to `out`. `location` names the module's file (NULL for none): errors cite it, and its directory is
/// searched for relative imports. `material` selects an exported material by name (NULL for the module's only one:
/// SHADEWIRE_ERROR_MATERIAL when it has several). `paths` holds `path_count` directories of the MDL search path, where
/// `import` finds modules (NULL and 0 for none; the standard modules and `::base` are built in). An MDL material is a
/// closure tree, so `options` must accept the `surface` model: the defaults (`options` NULL) refuse it. The IR is not
/// optimized; the front end's warnings are read with `shadewire_ir_warnings`. `*out` is written only on success.
///
/// # Safety
/// `text` must hold `len` readable bytes (NULL is reported, not dereferenced); `location` and `material` NULL or valid
/// NUL-terminated strings; `options` NULL or valid (see `shadewire_ir_options`); `paths` NULL or holding `path_count`
/// pointers, each a valid NUL-terminated string (NULL entries are reported); `out` valid for writing a pointer; `error`
/// NULL or valid for writing a pointer.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn shadewire_mdl_to_ir(
    text: *const c_char,
    len: usize,
    location: *const c_char,
    material: *const c_char,
    options: *const shadewire_ir_options,
    paths: *const *const c_char,
    path_count: usize,
    out: *mut *mut shadewire_ir,
    error: *mut *mut shadewire_error,
) -> shadewire_status {
    unsafe {
        guarded(
            error,
            AssertUnwindSafe(|| {
                if out.is_null() {
                    return Err(CapiError::Argument("out is NULL".to_string()));
                }
                let text = text_arg(text, len)?;
                let location = opt_cstr(location)?;
                let material = opt_cstr(material)?;
                let options = read_ir_options(options)?;
                let resolver = resolver(paths, path_count)?;
                let ir = shadewire_mdl::to_ir(text, location, material, &options, &resolver)?;
                *out = Box::into_raw(Box::new(shadewire_ir::new(ir)));
                Ok(())
            }),
        )
    }
}

/// The names of an MDL module's exported materials, in module order, written to `out`. `text`, `len`, `location`,
/// `paths` and `path_count` as `shadewire_mdl_to_ir`. `*out` is written only on success.
///
/// # Safety
/// As `shadewire_mdl_to_ir`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_mdl_material_names(
    text: *const c_char,
    len: usize,
    location: *const c_char,
    paths: *const *const c_char,
    path_count: usize,
    out: *mut *mut shadewire_strings,
    error: *mut *mut shadewire_error,
) -> shadewire_status {
    unsafe {
        guarded(
            error,
            AssertUnwindSafe(|| {
                if out.is_null() {
                    return Err(CapiError::Argument("out is NULL".to_string()));
                }
                let text = text_arg(text, len)?;
                let location = opt_cstr(location)?;
                let resolver = resolver(paths, path_count)?;
                let names = shadewire_mdl::material_names(text, location, &resolver)?;
                *out = Box::into_raw(Box::new(shadewire_strings::new(names)));
                Ok(())
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::{CStr, CString};
    use std::os::raw::c_char;

    use super::*;
    use crate::compiled::{
        shadewire_compile, shadewire_compiled_code, shadewire_compiled_free, shadewire_compiled_language,
    };
    use crate::error::{shadewire_error_free, shadewire_error_get_kind};
    use crate::error_kind::shadewire_error_kind::{self, SHADEWIRE_ERROR_INVALID_ARGUMENT};
    use crate::ir::{shadewire_ir_free, shadewire_ir_model};
    use crate::names::{shadewire_strings_count, shadewire_strings_free, shadewire_strings_get};
    use crate::options::{shadewire_ir_options, shadewire_ir_options_default};
    use crate::status::shadewire_status::{SHADEWIRE_ERROR, SHADEWIRE_OK};
    use crate::target::{SHADEWIRE_TARGET_MDL, shadewire_target};

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../shadewire-mdl/tests/fixtures");

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("{FIXTURES}/{name}.mdl")).unwrap()
    }

    /// IR options accepting the closure-tree models.
    fn lobes(models: &[CString; 2]) -> (shadewire_ir_options, [*const c_char; 2]) {
        let mut options = std::mem::MaybeUninit::uninit();
        unsafe { shadewire_ir_options_default(options.as_mut_ptr()) };
        (unsafe { options.assume_init() }, [models[0].as_ptr(), models[1].as_ptr()])
    }

    fn lower(
        text: &str,
        material: Option<&str>,
        paths: &[*const c_char],
    ) -> Result<*mut shadewire_ir, shadewire_error_kind> {
        let names = [CString::new("surface").unwrap(), CString::new("surface_unlit").unwrap()];
        let (mut options, list) = lobes(&names);
        options.models = list.as_ptr();
        options.model_count = list.len();
        let material = material.map(|m| CString::new(m).unwrap());
        let mut ir = std::ptr::null_mut();
        let mut error = std::ptr::null_mut();
        let status = unsafe {
            shadewire_mdl_to_ir(
                text.as_ptr().cast(),
                text.len(),
                std::ptr::null(),
                material.as_ref().map_or(std::ptr::null(), |m| m.as_ptr()),
                &options,
                paths.as_ptr(),
                paths.len(),
                &mut ir,
                &mut error,
            )
        };
        if status == SHADEWIRE_OK {
            return Ok(ir);
        }
        let kind = unsafe { shadewire_error_get_kind(error) };
        unsafe { shadewire_error_free(error) };
        Err(kind)
    }

    #[test]
    fn null_arguments_are_errors() {
        let text = fixture("diffuse");
        let mut error = std::ptr::null_mut();
        let status = unsafe {
            shadewire_mdl_to_ir(
                text.as_ptr().cast(),
                text.len(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                &mut error,
            )
        };
        assert_eq!(status, SHADEWIRE_ERROR);
        assert_eq!(unsafe { shadewire_error_get_kind(error) }, SHADEWIRE_ERROR_INVALID_ARGUMENT);
        unsafe { shadewire_error_free(error) };
        assert_eq!(lower(&text, None, &[std::ptr::null()]), Err(SHADEWIRE_ERROR_INVALID_ARGUMENT));
        let mut out = std::ptr::null_mut();
        let status = unsafe {
            shadewire_mdl_material_names(
                std::ptr::null(),
                0,
                std::ptr::null(),
                std::ptr::null(),
                0,
                &mut out,
                std::ptr::null_mut(),
            )
        };
        assert_eq!(status, SHADEWIRE_ERROR);
        assert!(out.is_null());
        let status = unsafe {
            shadewire_mdl_material_names(
                text.as_ptr().cast(),
                text.len(),
                std::ptr::null(),
                std::ptr::null(),
                3,
                &mut out,
                std::ptr::null_mut(),
            )
        };
        assert_eq!(status, SHADEWIRE_ERROR, "a NULL path list with a count");
    }

    #[test]
    fn a_fixture_lowers_to_a_closure_tree() {
        let ir = lower(&fixture("mixes"), Some("weighted"), &[]).unwrap();
        let model = unsafe { CStr::from_ptr(shadewire_ir_model(ir)) };
        assert_eq!(model.to_str().unwrap(), "surface");
        unsafe { shadewire_ir_free(ir) };
    }

    #[test]
    fn material_names_are_listed() {
        let text = fixture("mixes");
        let mut out = std::ptr::null_mut();
        let status = unsafe {
            shadewire_mdl_material_names(
                text.as_ptr().cast(),
                text.len(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                &mut out,
                std::ptr::null_mut(),
            )
        };
        assert_eq!(status, SHADEWIRE_OK);
        let names: Vec<String> = (0..unsafe { shadewire_strings_count(out) })
            .map(|i| unsafe { CStr::from_ptr(shadewire_strings_get(out, i)) }.to_str().unwrap().to_string())
            .collect();
        assert_eq!(names, ["normalized", "clamped", "unbounded", "color_normalized", "weighted", "tinted"]);
        unsafe { shadewire_strings_free(out) };
    }

    #[test]
    fn search_paths_resolve_imports() {
        let dir = std::env::temp_dir().join(format!("shadewire-capi-mdl-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        std::fs::write(dir.join("lib/tints.mdl"), "mdl 1.7;\nexport color red() = color(1.0, 0.0, 0.0);\n").unwrap();
        let text = "mdl 1.7;\nimport ::df::*;\nimport ::lib::tints::*;\n\
            export material m() = material(surface: material_surface(scattering: \
            df::diffuse_reflection_bsdf(tint: lib::tints::red())));\n";
        let path = CString::new(dir.to_str().unwrap()).unwrap();
        let ir = lower(text, None, &[path.as_ptr()]).unwrap();
        unsafe { shadewire_ir_free(ir) };
        assert!(lower(text, None, &[]).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_mdl_target_writes_a_module() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../shadewire-mtlx/tests/fixtures/porcelain.mtlx");
        let text = std::fs::read_to_string(path).unwrap();
        let options = shadewire_mtlx::IrOptions::with_models(&shadewire::LOBE_MODELS);
        let lowered = shadewire_mtlx::to_ir(&text, Some(path), None, &options, &Default::default()).unwrap();
        let ir = Box::into_raw(Box::new(crate::ir::shadewire_ir::new(lowered)));
        let mut compiled = std::ptr::null_mut();
        let target = shadewire_target { kind: SHADEWIRE_TARGET_MDL, version: 0 };
        let status = unsafe { shadewire_compile(ir, target, std::ptr::null(), &mut compiled, std::ptr::null_mut()) };
        assert_eq!(status, SHADEWIRE_OK);
        let code = unsafe { CStr::from_ptr(shadewire_compiled_code(compiled, std::ptr::null_mut())) };
        assert!(code.to_str().unwrap().starts_with("mdl 1.7;"));
        let language = unsafe { CStr::from_ptr(shadewire_compiled_language(compiled)) };
        assert_eq!(language.to_str().unwrap(), "mdl");
        unsafe { shadewire_compiled_free(compiled) };
        unsafe { shadewire_ir_free(ir) };
    }
}
