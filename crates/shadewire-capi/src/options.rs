//! Option structs passed from C, read as old callers may have built them.
//!
//! The structs only grow at the end: a new field starts at or after the current `sizeof`, so `struct_size` tells
//! which fields a caller knows. Tail padding is spelled out as zero-filled `reserved` bytes, ignored on read, so no
//! new field can land inside an old struct's size.

use std::ffi::{c_char, c_void};
use std::mem::MaybeUninit;

use shadewire_mtlx::Resolver;
use shadewire_nodes::IrOptions;
use shadewire_wgsl::CompileOptions;

use crate::resolver::{resolver, shadewire_resolve_fn};
use crate::strings::{ArgError, opt_cstr, string_list};

/// Options for MaterialX import. Set `struct_size` to `sizeof` the struct, or use `shadewire_mtlx_options_default`.
///
/// `models` NULL with `model_count` 0 is the default list; NULL with a non-zero count is an error; non-NULL is exactly
/// `model_count` NUL-terminated UTF-8 strings, and a count of 0 means no models (every material is then
/// NoTranslation). `working_colorspace` is NULL for the default. `values_as_uniforms` is true for any non-zero byte.
///
/// `reserved` fills the padding after `values_as_uniforms` and must be zero.
///
/// `resolve` reads `xi:include`s: NULL reads the file system relative to the including document's location (a
/// path), non-NULL is the host's resolver (see `shadewire_resolve_fn`), called with `resolve_user_data`. A document
/// with includes needs a location either way.
///
/// `distill` (true for any non-zero byte; false for a caller whose `struct_size` ends before it) distills a material
/// that would open into a closure tree to `open_pbr_surface` when `models` accepts it (`IrOptions::distill`).
/// `reserved2` fills the tail padding and must be zero. New fields are only appended at the end.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct shadewire_mtlx_options {
    pub struct_size: u32,
    pub models: *const *const c_char,
    pub model_count: usize,
    pub working_colorspace: *const c_char,
    pub values_as_uniforms: bool,
    pub reserved: [u8; 7],
    pub resolve: shadewire_resolve_fn,
    pub resolve_user_data: *mut c_void,
    pub distill: bool,
    pub reserved2: [u8; 7],
}

/// Options of a front end without includes (`shadewire_blender_to_ir`): the models the host accepts, the working
/// colorspace and whether authored values are uniforms, as `shadewire_mtlx_options` without the resolver. Set
/// `struct_size`, or use `shadewire_ir_options_default`.
///
/// `models`, `model_count`, `working_colorspace`, `values_as_uniforms` and `distill` read as in
/// `shadewire_mtlx_options`; `reserved` and `reserved2` fill padding and must be zero. New fields are only appended
/// at the end.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct shadewire_ir_options {
    pub struct_size: u32,
    pub models: *const *const c_char,
    pub model_count: usize,
    pub working_colorspace: *const c_char,
    pub values_as_uniforms: bool,
    pub reserved: [u8; 7],
    pub distill: bool,
    pub reserved2: [u8; 7],
}

/// Options for compilation (the keyword arguments of Python's `compile`). Set `struct_size` to `sizeof` the struct,
/// or use `shadewire_compile_options_default`. The booleans are true for any non-zero byte.
///
/// `group` and `first_binding` place the uniform block, textures and samplers. `reserve_uniform_binding` keeps
/// `first_binding` for the uniform block even when the material has no uniforms, so textures start at
/// `first_binding + 1` (for hosts that bind their own buffer there). `flip_v` flips V when sampling (images uploaded
/// top row first). `optimize` runs the optimizer first. `implicit_derivatives` false makes the code run in a compute
/// stage (level-0 samples, zero derivatives).
///
/// New fields are only appended at the end.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct shadewire_compile_options {
    pub struct_size: u32,
    pub group: u32,
    pub first_binding: u32,
    pub reserve_uniform_binding: bool,
    pub flip_v: bool,
    pub optimize: bool,
    pub implicit_derivatives: bool,
}

fn compile_defaults() -> shadewire_compile_options {
    let d = CompileOptions::default();
    shadewire_compile_options {
        struct_size: std::mem::size_of::<shadewire_compile_options>() as u32,
        group: d.group,
        first_binding: d.first_binding,
        reserve_uniform_binding: d.reserve_uniform_binding,
        flip_v: d.flip_v,
        optimize: d.optimize,
        implicit_derivatives: d.implicit_derivatives,
    }
}

fn mtlx_defaults() -> shadewire_mtlx_options {
    shadewire_mtlx_options {
        struct_size: std::mem::size_of::<shadewire_mtlx_options>() as u32,
        models: std::ptr::null(),
        model_count: 0,
        working_colorspace: std::ptr::null(),
        values_as_uniforms: IrOptions::default().values_as_uniforms,
        reserved: [0; 7],
        resolve: None,
        resolve_user_data: std::ptr::null_mut(),
        distill: IrOptions::default().distill,
        reserved2: [0; 7],
    }
}

fn ir_defaults() -> shadewire_ir_options {
    shadewire_ir_options {
        struct_size: std::mem::size_of::<shadewire_ir_options>() as u32,
        models: std::ptr::null(),
        model_count: 0,
        working_colorspace: std::ptr::null(),
        values_as_uniforms: IrOptions::default().values_as_uniforms,
        reserved: [0; 7],
        distill: IrOptions::default().distill,
        reserved2: [0; 7],
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

/// Reads MaterialX options, as the IR options and the include resolver; NULL gives the defaults.
///
/// # Safety
/// `ptr` must be NULL or point to `struct_size` readable bytes (at least 4); `models` must hold `model_count` valid
/// NUL-terminated strings, and `working_colorspace` must be NULL or a valid NUL-terminated string; `resolve` must be
/// NULL or a function as `shadewire_resolve_fn` describes.
pub(crate) unsafe fn read_mtlx(ptr: *const shadewire_mtlx_options) -> Result<(IrOptions, Resolver), ArgError> {
    let mut options = IrOptions::default();
    if ptr.is_null() {
        return Ok((options, Resolver::default()));
    }
    let raw = unsafe { overlay(ptr, mtlx_defaults()) }?;
    let raw = raw.as_ptr();
    unsafe {
        read_common(
            (&raw const (*raw).models).read(),
            (&raw const (*raw).model_count).read(),
            (&raw const (*raw).working_colorspace).read(),
            &raw const (*raw).values_as_uniforms,
            &mut options,
        )
    }?;
    options.distill = unsafe { flag(&raw const (*raw).distill) };
    let resolve = unsafe { (&raw const (*raw).resolve).read() };
    let resolver = resolver(resolve, unsafe { (&raw const (*raw).resolve_user_data).read() });
    Ok((options, resolver))
}

/// Reads the options of a front end without includes; NULL gives the defaults.
///
/// # Safety
/// `ptr` must be NULL or point to `struct_size` readable bytes (at least 4); `models` must hold `model_count` valid
/// NUL-terminated strings, and `working_colorspace` must be NULL or a valid NUL-terminated string.
pub(crate) unsafe fn read_ir_options(ptr: *const shadewire_ir_options) -> Result<IrOptions, ArgError> {
    let mut options = IrOptions::default();
    if ptr.is_null() {
        return Ok(options);
    }
    let raw = unsafe { overlay(ptr, ir_defaults()) }?;
    let raw = raw.as_ptr();
    unsafe {
        read_common(
            (&raw const (*raw).models).read(),
            (&raw const (*raw).model_count).read(),
            (&raw const (*raw).working_colorspace).read(),
            &raw const (*raw).values_as_uniforms,
            &mut options,
        )
    }?;
    options.distill = unsafe { flag(&raw const (*raw).distill) };
    Ok(options)
}

/// Reads the fields every front end's options share into `options`: the model list, the working colorspace and
/// `values_as_uniforms`.
///
/// # Safety
/// `models` must be NULL or hold `model_count` valid NUL-terminated strings (NULL entries are reported);
/// `working_colorspace` must be NULL or a valid NUL-terminated string; `values_as_uniforms` must point to a readable
/// byte.
unsafe fn read_common(
    models: *const *const c_char,
    model_count: usize,
    working_colorspace: *const c_char,
    values_as_uniforms: *const bool,
    options: &mut IrOptions,
) -> Result<(), ArgError> {
    if !models.is_null() {
        options.models = unsafe { string_list(models, model_count, "model") }?.into_iter().map(String::from).collect();
    } else if model_count > 0 {
        return Err(ArgError(format!("models is NULL but model_count is {model_count}")));
    }
    if let Some(space) = unsafe { opt_cstr(working_colorspace) }? {
        options.working_colorspace = space.to_string();
    }
    options.values_as_uniforms = unsafe { flag(values_as_uniforms) };
    Ok(())
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
            implicit_derivatives: flag(&raw const (*raw).implicit_derivatives),
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

/// Writes the default options of a front end without includes to `out` (no-op when NULL).
///
/// # Safety
/// `out` must be NULL or valid for writing a `shadewire_ir_options`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_ir_options_default(out: *mut shadewire_ir_options) {
    if !out.is_null() {
        unsafe { out.write(ir_defaults()) };
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
        let (read, _) = unsafe { read_mtlx(&m) }.unwrap();
        let d = IrOptions::default();
        assert_eq!(read.models, d.models);
        assert_eq!(read.working_colorspace, d.working_colorspace);
        assert_eq!(read.values_as_uniforms, d.values_as_uniforms);
    }

    #[test]
    fn the_structs_end_without_padding() {
        let compile_end = offset_of!(shadewire_compile_options, implicit_derivatives) + 1;
        assert_eq!(size_of::<shadewire_compile_options>(), compile_end);
        assert_eq!(offset_of!(shadewire_mtlx_options, reserved) + 7, offset_of!(shadewire_mtlx_options, resolve));
        let user_data_end = offset_of!(shadewire_mtlx_options, resolve_user_data) + size_of::<*mut c_void>();
        assert_eq!(offset_of!(shadewire_mtlx_options, distill), user_data_end);
        assert_eq!(size_of::<shadewire_mtlx_options>(), offset_of!(shadewire_mtlx_options, reserved2) + 7);
        #[cfg(target_pointer_width = "64")]
        assert_eq!((size_of::<shadewire_compile_options>(), size_of::<shadewire_mtlx_options>()), (16, 64));
    }

    #[test]
    fn defaults_zero_the_reserved_bytes() {
        let mut m = MaybeUninit::<shadewire_mtlx_options>::uninit();
        unsafe { m.as_mut_ptr().cast::<u8>().write_bytes(0xAA, size_of::<shadewire_mtlx_options>()) };
        unsafe { shadewire_mtlx_options_default(m.as_mut_ptr()) };
        let m = unsafe { m.assume_init() };
        assert_eq!((m.reserved, m.reserved2), ([0; 7], [0; 7]));
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
        set_bool_byte(&mut c, offset_of!(shadewire_compile_options, implicit_derivatives), 2);
        let read = unsafe { read_compile(c.as_ptr()) }.unwrap();
        let flags = [read.flip_v, read.optimize, read.reserve_uniform_binding, read.implicit_derivatives];
        assert_eq!(flags.map(u8::from), [1, 1, 1, 1]);
        let mut m = MaybeUninit::new(mtlx_default_struct());
        set_bool_byte(&mut m, offset_of!(shadewire_mtlx_options, values_as_uniforms), 2);
        let (read, _) = unsafe { read_mtlx(m.as_ptr()) }.unwrap();
        assert_eq!(u8::from(read.values_as_uniforms), 1);
    }

    #[test]
    fn null_reads_the_defaults() {
        assert_eq!(unsafe { read_compile(std::ptr::null()) }.unwrap(), CompileOptions::default());
        assert_eq!(unsafe { read_mtlx(std::ptr::null()) }.unwrap().0.models, IrOptions::default().models);
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
    fn implicit_derivatives_is_read() {
        let mut c = compile_default_struct();
        assert!(unsafe { read_compile(&c) }.unwrap().implicit_derivatives);
        c.implicit_derivatives = false;
        assert!(!unsafe { read_compile(&c) }.unwrap().implicit_derivatives);
        c.struct_size = offset_of!(shadewire_compile_options, implicit_derivatives) as u32;
        assert!(unsafe { read_compile(&c) }.unwrap().implicit_derivatives, "beyond struct_size: the default");
    }

    unsafe extern "C" fn unreachable_resolve(
        _: *mut c_void,
        _: *const c_char,
        _: *const c_char,
        _: *mut crate::resolver::shadewire_resolution,
    ) {
        std::process::abort();
    }

    #[test]
    fn an_old_caller_without_resolve_reads_the_file_system() {
        let mut m = mtlx_default_struct();
        m.resolve = Some(unreachable_resolve); // beyond struct_size: must not be read
        m.struct_size = offset_of!(shadewire_mtlx_options, resolve) as u32;
        let (_, resolver) = unsafe { read_mtlx(&m) }.unwrap();
        let err = resolver.resolve("no-such-dir/top.mtlx", "missing.mtlx").unwrap_err();
        assert!(err.contains("cannot read"), "{err}");
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
        let (read, _) = unsafe { read_mtlx(&m) }.unwrap();
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
        assert!(unsafe { read_mtlx(&m) }.unwrap().0.models.is_empty());
    }

    #[test]
    fn a_short_mtlx_struct_keeps_the_default_for_missing_fields() {
        let mut m = mtlx_default_struct();
        m.struct_size = offset_of!(shadewire_mtlx_options, values_as_uniforms) as u32;
        m.values_as_uniforms = !IrOptions::default().values_as_uniforms;
        let (read, _) = unsafe { read_mtlx(&m) }.unwrap();
        assert_eq!(read.values_as_uniforms, IrOptions::default().values_as_uniforms);
    }

    fn ir_default_struct() -> shadewire_ir_options {
        let mut o = MaybeUninit::<shadewire_ir_options>::uninit();
        unsafe { shadewire_ir_options_default(o.as_mut_ptr()) };
        unsafe { o.assume_init() }
    }

    #[test]
    fn ir_options_default_to_the_rust_defaults_and_end_without_padding() {
        let o = ir_default_struct();
        assert_eq!(o.struct_size as usize, size_of::<shadewire_ir_options>());
        let read = unsafe { read_ir_options(&o) }.unwrap();
        let d = IrOptions::default();
        assert_eq!(
            (read.models, read.working_colorspace, read.values_as_uniforms),
            (d.models.clone(), d.working_colorspace, d.values_as_uniforms)
        );
        assert_eq!(unsafe { read_ir_options(std::ptr::null()) }.unwrap().models, d.models);
        assert_eq!(offset_of!(shadewire_ir_options, distill), offset_of!(shadewire_ir_options, reserved) + 7);
        assert_eq!(size_of::<shadewire_ir_options>(), offset_of!(shadewire_ir_options, reserved2) + 7);
        #[cfg(target_pointer_width = "64")]
        assert_eq!(size_of::<shadewire_ir_options>(), 48);
        assert!(!read.distill && !o.distill);
        unsafe { shadewire_ir_options_default(std::ptr::null_mut()) };
    }

    #[test]
    fn ir_options_read_models_colorspace_and_flag() {
        let a = CString::new("surface").unwrap();
        let space = CString::new("srgb_texture").unwrap();
        let list = [a.as_ptr()];
        let mut o = ir_default_struct();
        o.models = list.as_ptr();
        o.model_count = 1;
        o.working_colorspace = space.as_ptr();
        o.values_as_uniforms = false;
        let read = unsafe { read_ir_options(&o) }.unwrap();
        assert_eq!(read.models, vec!["surface".to_string()]);
        assert_eq!(read.working_colorspace, "srgb_texture");
        assert!(!read.values_as_uniforms);
        o.struct_size = offset_of!(shadewire_ir_options, values_as_uniforms) as u32;
        assert_eq!(unsafe { read_ir_options(&o) }.unwrap().values_as_uniforms, IrOptions::default().values_as_uniforms);
        o.models = std::ptr::null();
        assert!(unsafe { read_ir_options(&o) }.unwrap_err().0.contains("models is NULL"));
        o.struct_size = 3;
        assert!(unsafe { read_ir_options(&o) }.is_err());
    }

    #[test]
    fn distill_is_read_and_an_old_caller_without_it_reads_false() {
        let mut m = mtlx_default_struct();
        m.distill = true;
        assert!(unsafe { read_mtlx(&m) }.unwrap().0.distill);
        m.struct_size = offset_of!(shadewire_mtlx_options, distill) as u32;
        assert!(!unsafe { read_mtlx(&m) }.unwrap().0.distill, "beyond struct_size: false");
        let mut o = ir_default_struct();
        let mut raw = MaybeUninit::new(o);
        set_bool_byte(&mut raw, offset_of!(shadewire_ir_options, distill), 2);
        assert!(unsafe { read_ir_options(raw.as_ptr()) }.unwrap().distill);
        o.distill = true;
        o.struct_size = offset_of!(shadewire_ir_options, distill) as u32;
        assert!(!unsafe { read_ir_options(&o) }.unwrap().distill, "beyond struct_size: false");
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
