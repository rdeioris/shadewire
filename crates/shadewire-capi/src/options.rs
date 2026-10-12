//! Option structs passed from C, read as old callers may have built them.

use std::ffi::c_char;

use shadewire_mtlx::MtlxOptions;
use shadewire_wgsl::CompileOptions;

use crate::strings::{ArgError, opt_cstr};

/// Options for MaterialX import. Set `struct_size` to `sizeof` the struct, or use `shadewire_mtlx_options_default`.
///
/// `models` NULL with `model_count` 0 is the default list; NULL with a non-zero count is an error; non-NULL is exactly
/// `model_count` NUL-terminated UTF-8 strings, and a count of 0 means no models (every material is then
/// NoTranslation). `working_colorspace` is NULL for the default. `values_as_uniforms` must be 0 or 1.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct shadewire_mtlx_options {
    pub struct_size: u32,
    pub models: *const *const c_char,
    pub model_count: usize,
    pub working_colorspace: *const c_char,
    pub values_as_uniforms: bool,
}

/// Options for compilation. Set `struct_size` to `sizeof` the struct, or use `shadewire_compile_options_default`.
/// The booleans must be 0 or 1.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct shadewire_compile_options {
    pub struct_size: u32,
    pub group: u32,
    pub first_binding: u32,
    pub flip_v: bool,
    pub optimize: bool,
}

fn compile_defaults() -> shadewire_compile_options {
    let d = CompileOptions::default();
    shadewire_compile_options {
        struct_size: std::mem::size_of::<shadewire_compile_options>() as u32,
        group: d.group,
        first_binding: d.first_binding,
        flip_v: d.flip_v,
        optimize: d.optimize,
    }
}

fn mtlx_defaults() -> shadewire_mtlx_options {
    shadewire_mtlx_options {
        struct_size: std::mem::size_of::<shadewire_mtlx_options>() as u32,
        models: std::ptr::null(),
        model_count: 0,
        working_colorspace: std::ptr::null(),
        values_as_uniforms: MtlxOptions::default().values_as_uniforms,
    }
}

/// Copies the first `min(struct_size, size_of::<T>())` bytes at `ptr` over `base`.
unsafe fn overlay<T: Copy>(ptr: *const T, base: T) -> Result<T, ArgError> {
    let declared = unsafe { std::ptr::read_unaligned(ptr.cast::<u32>()) } as usize;
    if declared < 4 {
        return Err(ArgError(format!("options struct_size {declared} is too small")));
    }
    let mut value = base;
    let len = declared.min(std::mem::size_of::<T>());
    unsafe { std::ptr::copy_nonoverlapping(ptr.cast::<u8>(), (&raw mut value).cast::<u8>(), len) };
    Ok(value)
}

/// Reads MaterialX options; NULL gives the defaults.
///
/// # Safety
/// `ptr` must be NULL or point to `struct_size` readable bytes (at least 4); `models` must hold `model_count` valid
/// NUL-terminated strings, and `working_colorspace` must be NULL or a valid NUL-terminated string.
pub(crate) unsafe fn read_mtlx(ptr: *const shadewire_mtlx_options) -> Result<MtlxOptions, ArgError> {
    let mut options = MtlxOptions::default();
    if ptr.is_null() {
        return Ok(options);
    }
    let raw = unsafe { overlay(ptr, mtlx_defaults()) }?;
    if raw.models.is_null() && raw.model_count > 0 {
        return Err(ArgError(format!("models is NULL but model_count is {}", raw.model_count)));
    }
    if !raw.models.is_null() {
        let mut models = Vec::new();
        for i in 0..raw.model_count {
            let entry = unsafe { *raw.models.add(i) };
            let text = unsafe { opt_cstr(entry) }?.ok_or_else(|| ArgError(format!("model {i} is NULL")))?;
            models.push(text.to_string());
        }
        options.models = models;
    }
    if let Some(space) = unsafe { opt_cstr(raw.working_colorspace) }? {
        options.working_colorspace = space.to_string();
    }
    options.values_as_uniforms = raw.values_as_uniforms;
    Ok(options)
}

/// Reads compile options; NULL gives the defaults.
///
/// # Safety
/// `ptr` must be NULL or point to `struct_size` readable bytes (at least 4).
pub(crate) unsafe fn read_compile(ptr: *const shadewire_compile_options) -> Result<CompileOptions, ArgError> {
    if ptr.is_null() {
        return Ok(CompileOptions::default());
    }
    let raw = unsafe { overlay(ptr, compile_defaults()) }?;
    Ok(CompileOptions {
        group: raw.group,
        first_binding: raw.first_binding,
        flip_v: raw.flip_v,
        optimize: raw.optimize,
    })
}

/// Writes the default MaterialX options to `out` (no-op when NULL).
///
/// # Safety
/// `out` must be NULL or valid for writing a `shadewire_mtlx_options`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_mtlx_options_default(out: *mut shadewire_mtlx_options) {
    if !out.is_null() {
        unsafe { out.write(mtlx_defaults()) };
    }
}

/// Writes the default compile options to `out` (no-op when NULL).
///
/// # Safety
/// `out` must be NULL or valid for writing a `shadewire_compile_options`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_compile_options_default(out: *mut shadewire_compile_options) {
    if !out.is_null() {
        unsafe { out.write(compile_defaults()) };
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::CString;
    use std::mem::{MaybeUninit, offset_of, size_of};

    use super::*;

    fn compile_default_struct() -> shadewire_compile_options {
        let mut o = MaybeUninit::<shadewire_compile_options>::uninit();
        unsafe { shadewire_compile_options_default(o.as_mut_ptr()) };
        unsafe { o.assume_init() }
    }

    fn mtlx_default_struct() -> shadewire_mtlx_options {
        let mut o = MaybeUninit::<shadewire_mtlx_options>::uninit();
        unsafe { shadewire_mtlx_options_default(o.as_mut_ptr()) };
        unsafe { o.assume_init() }
    }

    #[test]
    fn defaults_round_trip_to_the_rust_defaults() {
        let c = compile_default_struct();
        assert_eq!(c.struct_size as usize, size_of::<shadewire_compile_options>());
        assert_eq!(unsafe { read_compile(&c) }.unwrap(), CompileOptions::default());
        let m = mtlx_default_struct();
        assert_eq!(m.struct_size as usize, size_of::<shadewire_mtlx_options>());
        let read = unsafe { read_mtlx(&m) }.unwrap();
        let d = MtlxOptions::default();
        assert_eq!(read.models, d.models);
        assert_eq!(read.working_colorspace, d.working_colorspace);
        assert_eq!(read.values_as_uniforms, d.values_as_uniforms);
    }

    #[test]
    fn null_reads_the_defaults() {
        assert_eq!(unsafe { read_compile(std::ptr::null()) }.unwrap(), CompileOptions::default());
        assert_eq!(unsafe { read_mtlx(std::ptr::null()) }.unwrap().models, MtlxOptions::default().models);
    }

    #[test]
    fn default_functions_ignore_null() {
        unsafe { shadewire_compile_options_default(std::ptr::null_mut()) };
        unsafe { shadewire_mtlx_options_default(std::ptr::null_mut()) };
    }

    #[test]
    fn an_old_caller_without_optimize_gets_the_default() {
        let mut c = compile_default_struct();
        c.struct_size = offset_of!(shadewire_compile_options, optimize) as u32;
        c.group = 5;
        c.flip_v = false;
        c.optimize = false; // beyond struct_size: must not be read
        let read = unsafe { read_compile(&c) }.unwrap();
        assert_eq!((read.group, read.flip_v, read.optimize), (5, false, CompileOptions::default().optimize));
    }

    #[test]
    fn struct_size_below_four_is_an_error() {
        let mut c = compile_default_struct();
        c.struct_size = 0;
        assert!(unsafe { read_compile(&c) }.is_err());
        let mut m = mtlx_default_struct();
        m.struct_size = 3;
        assert!(unsafe { read_mtlx(&m) }.is_err());
    }

    #[test]
    fn models_and_colorspace_are_read() {
        let a = CString::new("standard_surface").unwrap();
        let b = CString::new("gltf_pbr").unwrap();
        let space = CString::new("srgb_texture").unwrap();
        let list = [a.as_ptr(), b.as_ptr()];
        let mut m = mtlx_default_struct();
        m.models = list.as_ptr();
        m.model_count = 2;
        m.working_colorspace = space.as_ptr();
        m.values_as_uniforms = false;
        let read = unsafe { read_mtlx(&m) }.unwrap();
        assert_eq!(read.models, vec!["standard_surface".to_string(), "gltf_pbr".to_string()]);
        assert_eq!(read.working_colorspace, "srgb_texture");
        assert!(!read.values_as_uniforms);
    }

    #[test]
    fn a_null_model_entry_is_an_error() {
        let a = CString::new("gltf_pbr").unwrap();
        let list = [a.as_ptr(), std::ptr::null()];
        let mut m = mtlx_default_struct();
        m.models = list.as_ptr();
        m.model_count = 2;
        assert!(unsafe { read_mtlx(&m) }.is_err());
    }

    #[test]
    fn an_invalid_utf8_model_is_an_error() {
        let bad = CString::new(vec![0xffu8]).unwrap();
        let list = [bad.as_ptr()];
        let mut m = mtlx_default_struct();
        m.models = list.as_ptr();
        m.model_count = 1;
        assert!(unsafe { read_mtlx(&m) }.is_err());
    }

    #[test]
    fn a_huge_count_fails_on_the_first_bad_entry_without_allocating() {
        let list = [std::ptr::null::<c_char>()];
        let mut m = mtlx_default_struct();
        m.models = list.as_ptr();
        m.model_count = 1_000_000;
        assert!(unsafe { read_mtlx(&m) }.unwrap_err().0.contains("model 0"));
    }

    #[test]
    fn null_models_with_a_count_is_an_error_and_empty_list_means_no_models() {
        let mut m = mtlx_default_struct();
        m.model_count = 3;
        assert!(unsafe { read_mtlx(&m) }.unwrap_err().0.contains("models is NULL"));
        let list: [*const c_char; 0] = [];
        m.models = list.as_ptr();
        m.model_count = 0;
        assert!(unsafe { read_mtlx(&m) }.unwrap().models.is_empty());
    }

    #[test]
    fn a_short_mtlx_struct_keeps_the_default_for_missing_fields() {
        let mut m = mtlx_default_struct();
        m.struct_size = offset_of!(shadewire_mtlx_options, values_as_uniforms) as u32;
        m.values_as_uniforms = !MtlxOptions::default().values_as_uniforms;
        let read = unsafe { read_mtlx(&m) }.unwrap();
        assert_eq!(read.values_as_uniforms, MtlxOptions::default().values_as_uniforms);
    }

    #[test]
    fn a_larger_struct_size_is_tolerated() {
        let mut c = compile_default_struct();
        c.struct_size = 1000;
        c.group = 7;
        assert_eq!(unsafe { read_compile(&c) }.unwrap().group, 7);
        let mut m = mtlx_default_struct();
        m.struct_size = 1000;
        assert!(unsafe { read_mtlx(&m) }.is_ok());
    }
}
