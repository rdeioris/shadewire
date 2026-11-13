//! The NVIDIA MDL front end (Python's `shadewire.mdl`): `shadewire_mdl_to_ir`, `shadewire_mdl_to_ir_args`,
//! `shadewire_mdl_material_names`. The MDL back end is `shadewire_compile` with `SHADEWIRE_TARGET_MDL`.

use std::os::raw::c_char;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;

use shadewire_mdl::{MdlArgValue, MdlArgument, Resolver};
use shadewire_nodes::document::builder::InputValue;

use crate::builder::{read_value, shadewire_value};
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

/// An argument of an MDL material, in place of its parameter `name`'s default (NUL-terminated UTF-8). `value` converts
/// to the parameter's declared type by MDL's rules: SHADEWIRE_VALUE_FLOATS of count 1 to a `float`, of count 2 to 4 to
/// a `float2` to `float4`, of count 3 to a `color`; SHADEWIRE_VALUE_INTEGER to an `int`, to the enum value with that
/// integer, or to a `float`; SHADEWIRE_VALUE_BOOLEAN to a `bool`; SHADEWIRE_VALUE_STRING to a `string`. With
/// `is_texture`, the SHADEWIRE_VALUE_STRING is a texture's path and the argument a `texture_2d(path, gamma)`, its gamma
/// given by `colorspace`: "sRGB" is `::tex::gamma_srgb`, "raw" `::tex::gamma_linear`, NULL, "auto" or any other
/// `::tex::gamma_default`. `colorspace` is read only with `is_texture`. A USD `Shader` prim's `inputs:*` map onto it:
/// `float`, `color3f`, `int` (enums too), `bool`, and `asset` (a texture, its `colorSpace` metadata the colorspace).
///
/// Versioning: an array of these is passed with its element size, `sizeof(shadewire_mdl_arg)` as the caller compiled
/// it. A later version may only append fields; the library steps through the array by the caller's size and reads the
/// fields it knows, so a caller built against an older (smaller) layout is refused rather than misread.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct shadewire_mdl_arg {
    pub name: *const c_char,
    pub value: shadewire_value,
    pub is_texture: bool,
    pub colorspace: *const c_char,
}

/// The `count` arguments at `args` (NULL and 0 for none), `size` bytes apart. Fields are read one by one,
/// `is_texture` as its byte. A `size` smaller than this version's `shadewire_mdl_arg`, or not a multiple of its
/// alignment, is an error; any bytes past the fields this version knows are ignored.
///
/// # Safety
/// `args` must be NULL or hold `count` readable elements of `size` bytes, each starting with a `shadewire_mdl_arg`
/// whose pointers are valid as its doc comment says.
pub(crate) unsafe fn read_args(
    args: *const shadewire_mdl_arg,
    count: usize,
    size: usize,
) -> Result<Vec<MdlArgument>, ArgError> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let known = std::mem::size_of::<shadewire_mdl_arg>();
    if size < known {
        return Err(ArgError(format!(
            "arg_size is {size}, smaller than sizeof(shadewire_mdl_arg), {known}: built against another header?"
        )));
    }
    let align = std::mem::align_of::<shadewire_mdl_arg>();
    if !size.is_multiple_of(align) {
        return Err(ArgError(format!("arg_size is {size}, not a multiple of the alignment, {align}")));
    }
    if args.is_null() {
        return Err(ArgError(format!("args is NULL but the count is {count}")));
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let arg = unsafe { args.cast::<u8>().add(i * size).cast::<shadewire_mdl_arg>() };
        let name = unsafe { opt_cstr((&raw const (*arg).name).read()) }?
            .ok_or_else(|| ArgError(format!("argument {i}: name is NULL")))?;
        let bad = |message: String| ArgError(format!("argument {name}: {message}"));
        let is_texture = unsafe { (&raw const (*arg).is_texture).cast::<u8>().read() } != 0;
        let value = unsafe { read_value(&raw const (*arg).value) }.map_err(|e| bad(e.0))?;
        let value = match value {
            InputValue::Text(path) if is_texture => {
                let colorspace = unsafe { opt_cstr((&raw const (*arg).colorspace).read()) }.map_err(|e| bad(e.0))?;
                MdlArgValue::Texture { path: path.to_string(), colorspace: colorspace.map(str::to_string) }
            }
            _ if is_texture => return Err(bad("a texture's path is a SHADEWIRE_VALUE_STRING".to_string())),
            InputValue::Floats(f) if f.iter().any(|x| !x.is_finite()) => {
                return Err(bad("a float is not finite".to_string()));
            }
            InputValue::Floats(f) => MdlArgValue::Float(f.to_vec()),
            InputValue::Integer(i) => MdlArgValue::Int(i),
            InputValue::Boolean(b) => MdlArgValue::Bool(b),
            InputValue::Text(s) => MdlArgValue::String(s.to_string()),
        };
        out.push(MdlArgument { name: name.to_string(), value });
    }
    Ok(out)
}

/// Lowers material `material` of the MDL module `text` (`len` bytes of UTF-8, not NUL-terminated) to the IR and writes
/// the handle to `out`. `location` names the module's file (NULL for none): errors cite it, and its directory is
/// searched for relative imports. `material` selects an exported material by name (NULL for the module's only one:
/// SHADEWIRE_ERROR_MATERIAL when it has several). `paths` holds `path_count` directories of the MDL search path, where
/// `import` finds modules (NULL and 0 for none; the standard modules and `::base` are built in). An MDL material is a
/// closure tree, so `options` must accept the `surface` model, or accept `open_pbr_surface` with `distill` set (the
/// tree then comes out distilled to it): the defaults (`options` NULL) refuse it. The IR is not
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

/// As `shadewire_mdl_to_ir`, the `arg_count` arguments at `args` (NULL and 0 for none; see `shadewire_mdl_arg`) in
/// place of the material's parameter defaults before it is evaluated, so what they decide folds: a texture argument
/// makes `tex::texture_isvalid` true. With `values_as_uniforms`, a float, vector, color or bool argument is the default
/// of its parameter's uniform; ints, enums, strings and textures stay constants. An argument naming no parameter, or
/// one that does not convert to its parameter's type, is an error naming the parameter (and both types).
/// `shadewire_mdl_to_ir` is this call with no arguments.
///
/// `arg_size` is `sizeof(shadewire_mdl_arg)` as the caller compiled it: the library steps through `args` by it and
/// reads only the fields it knows. With `arg_count` above 0, an `arg_size` smaller than this version's struct (or not a
/// multiple of its alignment) is SHADEWIRE_ERROR_INVALID_ARGUMENT; with `arg_count` 0 it is not read.
///
/// # Safety
/// As `shadewire_mdl_to_ir`; `args` NULL or holding `arg_count` readable elements of `arg_size` bytes, each starting
/// with a `shadewire_mdl_arg` whose `name` is a valid NUL-terminated string (NULL is reported), `value` valid (see
/// `shadewire_value`), and `colorspace` NULL or a valid NUL-terminated string.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn shadewire_mdl_to_ir_args(
    text: *const c_char,
    len: usize,
    location: *const c_char,
    material: *const c_char,
    options: *const shadewire_ir_options,
    paths: *const *const c_char,
    path_count: usize,
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
                let text = text_arg(text, len)?;
                let location = opt_cstr(location)?;
                let material = opt_cstr(material)?;
                let options = read_ir_options(options)?;
                let resolver = resolver(paths, path_count)?;
                let args = read_args(args, arg_count, arg_size)?;
                let ir = shadewire_mdl::to_ir_with_args(text, location, material, &args, &options, &resolver)?;
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
    fn mdl_args_are_read_and_checked() {
        use crate::builder::{SHADEWIRE_VALUE_FLOATS, SHADEWIRE_VALUE_STRING};
        const SIZE: usize = std::mem::size_of::<shadewire_mdl_arg>();
        let name = CString::new("tex").unwrap();
        let path = CString::new("oak.png").unwrap();
        let raw = CString::new("raw").unwrap();
        let value = |kind, floats: &[f32]| shadewire_value {
            kind,
            floats: floats.as_ptr(),
            count: floats.len(),
            integer: 0,
            boolean: false,
            string: path.as_ptr(),
        };
        let texture = shadewire_mdl_arg {
            name: name.as_ptr(),
            value: value(SHADEWIRE_VALUE_STRING, &[]),
            is_texture: true,
            colorspace: raw.as_ptr(),
        };
        let read = unsafe { read_args(&texture, 1, SIZE) }.unwrap();
        let expected = MdlArgValue::Texture { path: "oak.png".into(), colorspace: Some("raw".into()) };
        assert_eq!(read, [MdlArgument { name: "tex".into(), value: expected.clone() }]);
        assert_eq!(unsafe { read_args(std::ptr::null(), 0, 0) }.unwrap(), [], "arg_size is not read without arguments");
        assert!(unsafe { read_args(std::ptr::null(), 1, SIZE) }.is_err());

        // A caller of a later, larger layout: the library steps by its size and reads the fields it knows.
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct Later {
            arg: shadewire_mdl_arg,
            appended: u64,
        }
        let later = [Later { arg: texture, appended: u64::MAX }; 2];
        let read = unsafe { read_args(later.as_ptr().cast(), 2, std::mem::size_of::<Later>()) }.unwrap();
        assert_eq!(read.len(), 2);
        assert!(read.iter().all(|a| a.name == "tex" && a.value == expected), "{read:?}");

        // An older, smaller layout, or a stride off the alignment, is refused.
        for size in [0, SIZE - std::mem::align_of::<shadewire_mdl_arg>(), SIZE + 1] {
            let err = unsafe { read_args(&texture, 1, size) }.unwrap_err();
            assert!(err.0.contains("arg_size"), "{size}: {}", err.0);
        }
        let unnamed = shadewire_mdl_arg { name: std::ptr::null(), ..texture };
        assert!(unsafe { read_args(&unnamed, 1, SIZE) }.is_err());
        let floats = shadewire_mdl_arg { value: value(SHADEWIRE_VALUE_FLOATS, &[1.0]), ..texture };
        assert!(unsafe { read_args(&floats, 1, SIZE) }.is_err(), "a texture of floats");
        let nan = shadewire_mdl_arg { value: value(SHADEWIRE_VALUE_FLOATS, &[f32::NAN]), is_texture: false, ..texture };
        let err = unsafe { read_args(&nan, 1, SIZE) }.unwrap_err();
        assert!(err.0.contains("tex") && err.0.contains("finite"), "{}", err.0);
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
