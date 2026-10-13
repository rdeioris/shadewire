//! Option structs passed from C, read as old callers may have built them.
//!
//! The structs only grow at the end: a new field starts at or after the current `sizeof`, so `struct_size` tells
//! which fields a caller knows. Tail padding is spelled out as zero-filled `reserved` bytes, ignored on read, so no
//! new field can land inside an old struct's size.

use std::ffi::c_char;
use std::mem::MaybeUninit;

use shadewire_mtlx::MtlxOptions;
use shadewire_wgsl::CompileOptions;

use crate::strings::{ArgError, opt_cstr};

/// Options for MaterialX import. Set `struct_size` to `sizeof` the struct, or use `shadewire_mtlx_options_default`.
///
/// `models` NULL with `model_count` 0 is the default list; NULL with a non-zero count is an error; non-NULL is exactly
/// `model_count` NUL-terminated UTF-8 strings, and a count of 0 means no models (every material is then
/// NoTranslation). `working_colorspace` is NULL for the default. `values_as_uniforms` is true for any non-zero byte.
///
/// `reserved` fills the tail padding and must be zero; new fields are only appended after it.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct shadewire_mtlx_options {
    pub struct_size: u32,
    pub models: *const *const c_char,
    pub model_count: usize,
    pub working_colorspace: *const c_char,
    pub values_as_uniforms: bool,
    pub reserved: [u8; 7],
}

/// Options for compilation. Set `struct_size` to `sizeof` the struct, or use `shadewire_compile_options_default`.
/// The booleans are true for any non-zero byte.
///
/// `reserve_uniform_binding` keeps `first_binding` for the uniform block even when the material has no uniforms, so
/// textures start at `first_binding + 1` (for hosts that bind their own buffer there); a caller whose `struct_size`
/// ends before it gets false.
///
/// `reserved` and `reserved_tail` fill padding and must be zero; new fields are only appended after `reserved_tail`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct shadewire_compile_options {
    pub struct_size: u32,
    pub group: u32,
    pub first_binding: u32,
    pub flip_v: bool,
    pub optimize: bool,
    pub reserved: [u8; 2],
    pub reserve_uniform_binding: bool,
    pub reserved_tail: [u8; 3],
}

fn compile_defaults() -> shadewire_compile_options {
    let d = CompileOptions::default();
    shadewire_compile_options {
        struct_size: std::mem::size_of::<shadewire_compile_options>() as u32,
        group: d.group,
        first_binding: d.first_binding,
        flip_v: d.flip_v,
        optimize: d.optimize,
        reserved: [0; 2],
        reserve_uniform_binding: d.reserve_uniform_binding,
        reserved_tail: [0; 3],
    }
}

fn mtlx_defaults() -> shadewire_mtlx_options {
    shadewire_mtlx_options {
        struct_size: std::mem::size_of::<shadewire_mtlx_options>() as u32,
        models: std::ptr::null(),
        model_count: 0,
        working_colorspace: std::ptr::null(),
        values_as_uniforms: MtlxOptions::default().values_as_uniforms,
        reserved: [0; 7],
    }
}

/// Copies the first `min(struct_size, size_of::<T>())` bytes at `ptr` over `base`.
///
/// The result stays behind `MaybeUninit`: a caller may store any byte in a `bool` field, so fields are read one by one
/// through raw pointers, and `bool` fields with `flag`.
unsafe fn overlay<T: Copy>(ptr: *const T, base: T) -> Result<MaybeUninit<T>, ArgError> {
    let declared = unsafe { std::ptr::read_unaligned(ptr.cast::<u32>()) } as usize;
    if declared < 4 {
        return Err(ArgError(format!("options struct_size {declared} is too small")));
    }
    let mut value = MaybeUninit::new(base);
    let len = declared.min(std::mem::size_of::<T>());
    unsafe { std::ptr::copy_nonoverlapping(ptr.cast::<u8>(), value.as_mut_ptr().cast::<u8>(), len) };
    Ok(value)
}

/// Reads a C `bool` field as its byte: any non-zero byte is true.
///
/// # Safety
/// `field` must point to a readable byte.
unsafe fn flag(field: *const bool) -> bool {
    unsafe { field.cast::<u8>().read() != 0 }
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
    let raw = raw.as_ptr();
    let models = unsafe { (&raw const (*raw).models).read() };
    let model_count = unsafe { (&raw const (*raw).model_count).read() };
    let working_colorspace = unsafe { (&raw const (*raw).working_colorspace).read() };
    if models.is_null() && model_count > 0 {
        return Err(ArgError(format!("models is NULL but model_count is {model_count}")));
    }
    if !models.is_null() {
        let mut list = Vec::new();
        for i in 0..model_count {
            let entry = unsafe { *models.add(i) };
            let text = unsafe { opt_cstr(entry) }?.ok_or_else(|| ArgError(format!("model {i} is NULL")))?;
            list.push(text.to_string());
        }
        options.models = list;
    }
    if let Some(space) = unsafe { opt_cstr(working_colorspace) }? {
        options.working_colorspace = space.to_string();
    }
    options.values_as_uniforms = unsafe { flag(&raw const (*raw).values_as_uniforms) };
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
    let raw = raw.as_ptr();
    unsafe {
        Ok(CompileOptions {
            group: (&raw const (*raw).group).read(),
            first_binding: (&raw const (*raw).first_binding).read(),
            reserve_uniform_binding: flag(&raw const (*raw).reserve_uniform_binding),
            flip_v: flag(&raw const (*raw).flip_v),
            optimize: flag(&raw const (*raw).optimize),
            implicit_derivatives: true,
        })
    }
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
    fn the_reserved_bytes_end_each_struct() {
        let compile_end = offset_of!(shadewire_compile_options, reserved_tail) + 3;
        assert_eq!(size_of::<shadewire_compile_options>(), compile_end);
        // The 16-byte struct of the first release ended with `reserved`; the new field starts after it.
        assert_eq!(offset_of!(shadewire_compile_options, reserved) + 2, 16);
        assert_eq!(offset_of!(shadewire_compile_options, reserve_uniform_binding), 16);
        let mtlx_end = offset_of!(shadewire_mtlx_options, reserved) + 7;
        assert_eq!(size_of::<shadewire_mtlx_options>(), mtlx_end);
        #[cfg(target_pointer_width = "64")]
        assert_eq!((size_of::<shadewire_compile_options>(), size_of::<shadewire_mtlx_options>()), (20, 40));
    }

    #[test]
    fn defaults_zero_the_reserved_bytes() {
        let mut c = MaybeUninit::<shadewire_compile_options>::uninit();
        unsafe { c.as_mut_ptr().cast::<u8>().write_bytes(0xAA, size_of::<shadewire_compile_options>()) };
        unsafe { shadewire_compile_options_default(c.as_mut_ptr()) };
        let c = unsafe { c.assume_init() };
        assert_eq!((c.reserved, c.reserved_tail), ([0; 2], [0; 3]));
        let mut m = MaybeUninit::<shadewire_mtlx_options>::uninit();
        unsafe { m.as_mut_ptr().cast::<u8>().write_bytes(0xAA, size_of::<shadewire_mtlx_options>()) };
        unsafe { shadewire_mtlx_options_default(m.as_mut_ptr()) };
        assert_eq!(unsafe { m.assume_init() }.reserved, [0; 7]);
    }

    /// Stores byte `value` in the bool at `offset`; the struct stays behind `MaybeUninit`, so no invalid `bool` exists.
    fn set_bool_byte<T>(raw: &mut MaybeUninit<T>, offset: usize, value: u8) {
        unsafe { raw.as_mut_ptr().cast::<u8>().add(offset).write(value) };
    }

    #[test]
    fn a_bool_byte_of_two_reads_as_true() {
        let mut c = MaybeUninit::new(compile_default_struct());
        set_bool_byte(&mut c, offset_of!(shadewire_compile_options, flip_v), 2);
        set_bool_byte(&mut c, offset_of!(shadewire_compile_options, optimize), 2);
        set_bool_byte(&mut c, offset_of!(shadewire_compile_options, reserve_uniform_binding), 2);
        let read = unsafe { read_compile(c.as_ptr()) }.unwrap();
        assert_eq!((u8::from(read.flip_v), u8::from(read.optimize), u8::from(read.reserve_uniform_binding)), (1, 1, 1));
        let mut m = MaybeUninit::new(mtlx_default_struct());
        set_bool_byte(&mut m, offset_of!(shadewire_mtlx_options, values_as_uniforms), 2);
        let read = unsafe { read_mtlx(m.as_ptr()) }.unwrap();
        assert_eq!(u8::from(read.values_as_uniforms), 1);
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
    fn an_old_16_byte_caller_does_not_reserve_the_uniform_binding() {
        let mut c = compile_default_struct();
        c.struct_size = 16;
        c.reserve_uniform_binding = true; // beyond struct_size: must not be read
        assert!(!unsafe { read_compile(&c) }.unwrap().reserve_uniform_binding);
        c.struct_size = size_of::<shadewire_compile_options>() as u32;
        assert!(unsafe { read_compile(&c) }.unwrap().reserve_uniform_binding);
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
    fn a_huge_count_stops_at_the_first_null_entry() {
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
