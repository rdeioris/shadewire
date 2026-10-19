//! MDL modules parsed once and lowered as often as asked (`shadewire_mdl::Modules`): the opaque `shadewire_mdl_modules`
//! handle, with `shadewire_mdl_modules_to_ir` and `shadewire_mdl_modules_material_names`.

use std::os::raw::c_char;
use std::panic::AssertUnwindSafe;

use shadewire_mdl::{Modules, Resolver};

use crate::error::{CapiError, shadewire_error};
use crate::guard::guarded;
use crate::ir::shadewire_ir;
use crate::mdl::{read_args, shadewire_mdl_arg};
use crate::names::shadewire_strings;
use crate::options::{read_ir_options, read_mdl_modules, shadewire_ir_options, shadewire_mdl_modules_options};
use crate::status::shadewire_status;
use crate::strings::opt_cstr;

/// MDL modules on one search path, each parsed once with its imports, so the core library (`::base`, `::OmniPBR`,
/// ...) a stage's materials share is read once for all of them. Made by `shadewire_mdl_modules_new`, freed with
/// `shadewire_mdl_modules_free`.
///
/// A module is read from disk on its first use and kept: later edits to its file are not seen (make a new handle to
/// read them). A module that failed to load fails again, with the same error, on every later use.
///
/// Threads: any number of threads may call `shadewire_mdl_modules_to_ir` and `_material_names` on one handle at once
/// (loading a module not yet seen takes a lock; lowering runs side by side). Free it once, after every call is done.
pub struct shadewire_mdl_modules {
    modules: Modules,
}

/// A new, empty set of MDL modules searching `options`' path (NULL for none; see `shadewire_mdl_modules_options`),
/// written to `out`. `*out` is written only on success.
///
/// # Safety
/// `options` NULL or valid (see `shadewire_mdl_modules_options`); `out` valid for writing a pointer; `error` NULL or
/// valid for writing a pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_mdl_modules_new(
    options: *const shadewire_mdl_modules_options,
    out: *mut *mut shadewire_mdl_modules,
    error: *mut *mut shadewire_error,
) -> shadewire_status {
    unsafe {
        guarded(
            error,
            AssertUnwindSafe(|| {
                if out.is_null() {
                    return Err(CapiError::Argument("out is NULL".to_string()));
                }
                let paths = read_mdl_modules(options)?;
                let modules = Modules::new(Resolver::new(paths))?;
                *out = Box::into_raw(Box::new(shadewire_mdl_modules { modules }));
                Ok(())
            }),
        )
    }
}

/// As `shadewire_mdl_to_ir_args`, the module read through `modules`: `module` (NUL-terminated UTF-8, not NULL) is a
/// module name (`::OmniPBR`) found on the handle's search path, or the path of a `.mdl` file, which is then its
/// location as `shadewire_mdl_to_ir`'s `location` is. It and its imports are parsed on first use and shared by every
/// later call. `material`, `options`, `args`, `arg_count` and `arg_size` read as in `shadewire_mdl_to_ir_args`.
/// `*out` is written only on success.
///
/// # Safety
/// `modules` a live handle (NULL is reported); `module` NULL (reported) or a valid NUL-terminated string; the rest as
/// `shadewire_mdl_to_ir_args`.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn shadewire_mdl_modules_to_ir(
    modules: *const shadewire_mdl_modules,
    module: *const c_char,
    material: *const c_char,
    options: *const shadewire_ir_options,
    args: *const shadewire_mdl_arg,
    arg_count: usize,
    arg_size: usize,
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
                let modules = handle(modules)?;
                let module = opt_cstr(module)?.ok_or_else(|| CapiError::Argument("module is NULL".to_string()))?;
                let material = opt_cstr(material)?;
                let options = read_ir_options(options)?;
                let args = read_args(args, arg_count, arg_size)?;
                let ir = modules.to_ir_with_args(module, material, &args, &options)?;
                *out = Box::into_raw(Box::new(shadewire_ir::new(ir)));
                Ok(())
            }),
        )
    }
}

/// The names of the exported materials of `module` (as `shadewire_mdl_modules_to_ir` names it), in module order,
/// written to `out`. `*out` is written only on success.
///
/// # Safety
/// As `shadewire_mdl_modules_to_ir`; `out` valid for writing a pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_mdl_modules_material_names(
    modules: *const shadewire_mdl_modules,
    module: *const c_char,
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
                let modules = handle(modules)?;
                let module = opt_cstr(module)?.ok_or_else(|| CapiError::Argument("module is NULL".to_string()))?;
                let names = modules.material_names(module)?;
                *out = Box::into_raw(Box::new(shadewire_strings::new(names)));
                Ok(())
            }),
        )
    }
}

/// Frees `modules`; NULL is allowed.
///
/// # Safety
/// `modules` must be NULL or a handle from this library not yet freed, with no call on it still running.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_mdl_modules_free(modules: *mut shadewire_mdl_modules) {
    if !modules.is_null() {
        drop(unsafe { Box::from_raw(modules) });
    }
}

/// The modules behind `modules`, or an error when it is NULL.
///
/// # Safety
/// `modules` must be NULL or a live handle.
unsafe fn handle<'a>(modules: *const shadewire_mdl_modules) -> Result<&'a Modules, CapiError> {
    unsafe { modules.as_ref() }.map(|m| &m.modules).ok_or_else(|| CapiError::Argument("modules is NULL".to_string()))
}

#[cfg(test)]
mod tests {
    use std::ffi::{CStr, CString};

    use super::*;
    use crate::builder::{SHADEWIRE_VALUE_FLOATS, shadewire_value};
    use crate::error::{shadewire_error_free, shadewire_error_get_kind, shadewire_error_message};
    use crate::error_kind::shadewire_error_kind::SHADEWIRE_ERROR_INVALID_ARGUMENT;
    use crate::ir::{shadewire_ir_free, shadewire_ir_json};
    use crate::names::{shadewire_strings_count, shadewire_strings_free, shadewire_strings_get};
    use crate::options::{shadewire_ir_options_default, shadewire_mdl_modules_options_default};
    use crate::status::shadewire_status::{SHADEWIRE_ERROR, SHADEWIRE_OK};

    /// The handle may be shared across threads, as its doc comment says.
    #[test]
    fn the_handle_is_send_and_sync() {
        fn shared<T: Send + Sync>() {}
        shared::<shadewire_mdl_modules>();
    }

    /// A folder holding `lib/tints.mdl` and `stage/a.mdl`, `stage/b.mdl` importing it.
    fn stage(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("shadewire-capi-mdl-modules-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        std::fs::create_dir_all(dir.join("stage")).unwrap();
        std::fs::write(dir.join("lib/tints.mdl"), "mdl 1.7;\nexport color red() = color(1.0, 0.0, 0.0);\n").unwrap();
        for (name, k) in [("a", "0.25"), ("b", "0.75")] {
            let text = format!(
                "mdl 1.7;\nimport ::df::*;\nimport ::lib::tints::*;\n\
                 export material {name}(float k = {k}) = material(surface: material_surface(scattering: \
                 df::diffuse_reflection_bsdf(tint: lib::tints::red() * k)));\n"
            );
            std::fs::write(dir.join(format!("stage/{name}.mdl")), text).unwrap();
        }
        dir
    }

    fn new_modules(paths: &[*const c_char]) -> *mut shadewire_mdl_modules {
        let mut options = std::mem::MaybeUninit::uninit();
        unsafe { shadewire_mdl_modules_options_default(options.as_mut_ptr()) };
        let mut options = unsafe { options.assume_init() };
        options.paths = paths.as_ptr();
        options.path_count = paths.len();
        let mut modules = std::ptr::null_mut();
        let status = unsafe { shadewire_mdl_modules_new(&options, &mut modules, std::ptr::null_mut()) };
        assert_eq!(status, SHADEWIRE_OK);
        modules
    }

    /// Lowers `module` through `modules` with `args`: the IR's JSON, or the error's message.
    fn lower(
        modules: *const shadewire_mdl_modules,
        module: &str,
        args: &[shadewire_mdl_arg],
    ) -> Result<String, String> {
        let models = [CString::new("surface").unwrap(), CString::new("surface_unlit").unwrap()];
        let list = [models[0].as_ptr(), models[1].as_ptr()];
        let mut options = std::mem::MaybeUninit::uninit();
        unsafe { shadewire_ir_options_default(options.as_mut_ptr()) };
        let mut options = unsafe { options.assume_init() };
        options.models = list.as_ptr();
        options.model_count = list.len();
        let module = CString::new(module).unwrap();
        let mut ir = std::ptr::null_mut();
        let mut error = std::ptr::null_mut();
        let status = unsafe {
            shadewire_mdl_modules_to_ir(
                modules,
                module.as_ptr(),
                std::ptr::null(),
                &options,
                if args.is_empty() { std::ptr::null() } else { args.as_ptr() },
                args.len(),
                std::mem::size_of::<shadewire_mdl_arg>(),
                &mut ir,
                &mut error,
            )
        };
        if status != SHADEWIRE_OK {
            let message = unsafe { CStr::from_ptr(shadewire_error_message(error, std::ptr::null_mut())) };
            let message = message.to_string_lossy().into_owned();
            unsafe { shadewire_error_free(error) };
            return Err(message);
        }
        let json = unsafe { CStr::from_ptr(shadewire_ir_json(ir, std::ptr::null_mut())) };
        let json = json.to_string_lossy().into_owned();
        unsafe { shadewire_ir_free(ir) };
        Ok(json)
    }

    fn float_arg(name: &CString, value: &f32) -> shadewire_mdl_arg {
        shadewire_mdl_arg {
            name: name.as_ptr(),
            value: shadewire_value {
                kind: SHADEWIRE_VALUE_FLOATS,
                floats: value,
                count: 1,
                integer: 0,
                boolean: false,
                string: std::ptr::null(),
            },
            is_texture: false,
            colorspace: std::ptr::null(),
        }
    }

    #[test]
    fn files_and_names_lower_many_times_from_one_handle() {
        let dir = stage("many");
        let root = CString::new(dir.to_str().unwrap()).unwrap();
        let modules = new_modules(&[root.as_ptr()]);
        let a = dir.join("stage/a.mdl").to_string_lossy().into_owned();
        let first = lower(modules, &a, &[]).unwrap();
        for _ in 0..3 {
            assert_eq!(lower(modules, &a, &[]).unwrap(), first, "the same IR every time");
        }
        assert!(lower(modules, &dir.join("stage/b.mdl").to_string_lossy(), &[]).is_ok());
        assert!(lower(modules, "::stage::a", &[]).is_ok(), "a module name on the search path");

        // Arguments read as shadewire_mdl_to_ir_args reads them: an unknown name is an error naming it.
        let one = 1.0f32;
        let unknown = CString::new("no_such_input").unwrap();
        let err = lower(modules, &a, &[float_arg(&unknown, &one)]).unwrap_err();
        assert!(err.contains("no_such_input"), "{err}");
        let k = CString::new("k").unwrap();
        let with_k = lower(modules, &a, &[float_arg(&k, &one)]).unwrap();
        assert_ne!(with_k, first, "the argument replaces the default");

        let module = CString::new(a.as_str()).unwrap();
        let mut names = std::ptr::null_mut();
        let status =
            unsafe { shadewire_mdl_modules_material_names(modules, module.as_ptr(), &mut names, std::ptr::null_mut()) };
        assert_eq!(status, SHADEWIRE_OK);
        assert_eq!(unsafe { shadewire_strings_count(names) }, 1);
        assert_eq!(unsafe { CStr::from_ptr(shadewire_strings_get(names, 0)) }.to_str().unwrap(), "a");
        unsafe { shadewire_strings_free(names) };

        // The module was read once: removing its files does not change what later calls see.
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(lower(modules, &a, &[]).unwrap(), first);
        unsafe { shadewire_mdl_modules_free(modules) };
    }

    #[test]
    fn a_failed_load_fails_again_and_the_handle_stays_usable() {
        let dir = stage("failed");
        let modules = new_modules(&[]);
        let a = dir.join("stage/a.mdl").to_string_lossy().into_owned();
        let first = lower(modules, &a, &[]).unwrap_err();
        assert!(first.contains("::lib::tints"), "{first}");
        assert_eq!(lower(modules, &a, &[]).unwrap_err(), first);
        let missing = dir.join("stage/missing.mdl").to_string_lossy().into_owned();
        assert!(lower(modules, &missing, &[]).is_err());
        // The library itself still loads.
        std::fs::write(dir.join("stage/c.mdl"), "mdl 1.7;\nimport ::df::*;\nexport material c() = material();\n")
            .unwrap();
        assert!(lower(modules, &dir.join("stage/c.mdl").to_string_lossy(), &[]).is_ok());
        std::fs::remove_dir_all(&dir).unwrap();
        unsafe { shadewire_mdl_modules_free(modules) };
    }

    #[test]
    fn threads_lower_from_one_handle_at_once() {
        let dir = stage("threads");
        let root = CString::new(dir.to_str().unwrap()).unwrap();
        let modules = new_modules(&[root.as_ptr()]) as usize;
        let files: Vec<String> =
            ["a", "b"].iter().map(|n| dir.join(format!("stage/{n}.mdl")).to_string_lossy().into_owned()).collect();
        // What a handle of its own lowers each file to.
        let expected: Vec<String> = files
            .iter()
            .map(|f| {
                let own = new_modules(&[root.as_ptr()]);
                let ir = lower(own, f, &[]).unwrap();
                unsafe { shadewire_mdl_modules_free(own) };
                ir
            })
            .collect();
        std::thread::scope(|threads| {
            for i in 0..8 {
                let (file, expected) = (&files[i % 2], &expected[i % 2]);
                threads.spawn(move || {
                    for _ in 0..4 {
                        assert_eq!(&lower(modules as *const shadewire_mdl_modules, file, &[]).unwrap(), expected);
                    }
                });
            }
        });
        unsafe { shadewire_mdl_modules_free(modules as *mut shadewire_mdl_modules) };
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn null_arguments_are_errors() {
        let mut error = std::ptr::null_mut();
        let status = unsafe { shadewire_mdl_modules_new(std::ptr::null(), std::ptr::null_mut(), &mut error) };
        assert_eq!(status, SHADEWIRE_ERROR);
        assert_eq!(unsafe { shadewire_error_get_kind(error) }, SHADEWIRE_ERROR_INVALID_ARGUMENT);
        unsafe { shadewire_error_free(error) };
        let modules = new_modules(&[]);
        assert!(lower(std::ptr::null(), "::x", &[]).unwrap_err().contains("modules is NULL"));
        let mut ir = std::ptr::null_mut();
        let status = unsafe {
            shadewire_mdl_modules_to_ir(
                modules,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                0,
                &mut ir,
                std::ptr::null_mut(),
            )
        };
        assert_eq!(status, SHADEWIRE_ERROR, "a NULL module");
        assert!(ir.is_null());
        let mut names = std::ptr::null_mut();
        let status = unsafe {
            shadewire_mdl_modules_material_names(std::ptr::null(), std::ptr::null(), &mut names, std::ptr::null_mut())
        };
        assert_eq!(status, SHADEWIRE_ERROR);
        assert!(names.is_null());
        unsafe { shadewire_mdl_modules_free(modules) };
        unsafe { shadewire_mdl_modules_free(std::ptr::null_mut()) };
    }
}
