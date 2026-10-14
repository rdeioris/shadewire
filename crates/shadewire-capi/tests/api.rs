//! The C API driven through its `extern "C"` functions.

use std::ffi::{CStr, CString};
use std::ptr::{null, null_mut};

use shadewire_capi::*;

const WGSL: shadewire_target = shadewire_target { kind: SHADEWIRE_TARGET_WGSL, version: 0 };

fn default_example() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../vendor/materialx/resources/Materials/Examples/StandardSurface/standard_surface_default.mtlx"
    );
    std::fs::read_to_string(path).unwrap()
}

fn to_ir(text: &str) -> *mut shadewire_ir {
    to_ir_with(text, null())
}

fn to_ir_with(text: &str, options: *const shadewire_mtlx_options) -> *mut shadewire_ir {
    let mut ir = null_mut();
    let mut error = null_mut();
    let status =
        unsafe { shadewire_mtlx_to_ir(text.as_ptr().cast(), text.len(), null(), null(), options, &mut ir, &mut error) };
    assert_eq!(status, shadewire_status::SHADEWIRE_OK);
    assert!(error.is_null());
    ir
}

#[test]
fn the_default_example_lowers() {
    let ir = to_ir(&default_example());
    let model = unsafe { CStr::from_ptr(shadewire_ir_model(ir)) };
    assert_eq!(model.to_str().unwrap(), "standard_surface");
    let mut len = 0usize;
    let dump = unsafe { CStr::from_ptr(shadewire_ir_dump(ir, &mut len)) };
    assert!(dump.to_str().unwrap().starts_with("model standard_surface"));
    assert_eq!(len, dump.to_bytes().len());
    unsafe { shadewire_ir_free(ir) };
}

#[test]
fn malformed_xml_is_a_parse_error_with_a_location() {
    let text = "<materialx><nodegraph></materialx>";
    let path = CString::new("bad.mtlx").unwrap();
    let mut ir = null_mut();
    let mut error = null_mut();
    let status = unsafe {
        shadewire_mtlx_to_ir(text.as_ptr().cast(), text.len(), path.as_ptr(), null(), null(), &mut ir, &mut error)
    };
    assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
    assert!(ir.is_null());
    assert_eq!(unsafe { shadewire_error_get_kind(error) }, shadewire_error_kind::SHADEWIRE_ERROR_PARSE);
    assert_eq!(unsafe { CStr::from_ptr(shadewire_error_file(error)) }.to_str().unwrap(), "bad.mtlx");
    assert!(unsafe { shadewire_error_line(error) } > 0);
    unsafe { shadewire_error_free(error) };
}

#[test]
fn invalid_utf8_is_an_invalid_argument() {
    let bytes = [0xffu8, 0xfe];
    let mut ir = null_mut();
    let mut error = null_mut();
    let status = unsafe { shadewire_mtlx_to_ir(bytes.as_ptr().cast(), 2, null(), null(), null(), &mut ir, &mut error) };
    assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
    assert_eq!(unsafe { shadewire_error_get_kind(error) }, shadewire_error_kind::SHADEWIRE_ERROR_INVALID_ARGUMENT);
    unsafe { shadewire_error_free(error) };
}

#[test]
fn a_null_out_is_an_invalid_argument() {
    let text = default_example();
    let mut error = null_mut();
    let status = unsafe {
        shadewire_mtlx_to_ir(text.as_ptr().cast(), text.len(), null(), null(), null(), null_mut(), &mut error)
    };
    assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
    assert_eq!(unsafe { shadewire_error_get_kind(error) }, shadewire_error_kind::SHADEWIRE_ERROR_INVALID_ARGUMENT);
    unsafe { shadewire_error_free(error) };
}

#[test]
fn freeing_null_is_fine() {
    unsafe { shadewire_ir_free(null_mut()) };
}

fn compile(ir: *const shadewire_ir, options: *const shadewire_compile_options) -> *mut shadewire_compiled {
    let mut compiled = null_mut();
    let mut error = null_mut();
    let status = unsafe { shadewire_compile(ir, WGSL, options, &mut compiled, &mut error) };
    assert_eq!(status, shadewire_status::SHADEWIRE_OK);
    compiled
}

fn text(ptr: *const std::os::raw::c_char) -> String {
    unsafe { CStr::from_ptr(ptr) }.to_str().unwrap().to_string()
}

#[test]
fn the_default_example_compiles() {
    let ir = to_ir(&default_example());
    let compiled = compile(ir, null());
    let code = text(unsafe { shadewire_compiled_code(compiled, null_mut()) });
    assert!(code.contains("fn shadewire_eval_surface() -> StandardSurface"));
    assert_eq!(text(unsafe { shadewire_compiled_model_struct(compiled) }), "StandardSurface");
    assert_eq!(text(unsafe { shadewire_compiled_language(compiled) }), "wgsl");
    let json: serde_json::Value =
        serde_json::from_str(&text(unsafe { shadewire_compiled_reflection_json(compiled, null_mut()) })).unwrap();
    assert_eq!(json["model"], "standard_surface");
    assert!(text(unsafe { shadewire_compiled_ir_dump(compiled, null_mut()) }).starts_with("model standard_surface"));
    unsafe { shadewire_compiled_free(compiled) };
    unsafe { shadewire_ir_free(ir) };
}

#[test]
fn compile_options_reach_the_reflection() {
    let ir = to_ir(&default_example());
    let mut options = std::mem::MaybeUninit::<shadewire_compile_options>::uninit();
    unsafe { shadewire_compile_options_default(options.as_mut_ptr()) };
    let mut options = unsafe { options.assume_init() };
    options.group = 3;
    let compiled = compile(ir, &options);
    let json: serde_json::Value =
        serde_json::from_str(&text(unsafe { shadewire_compiled_reflection_json(compiled, null_mut()) })).unwrap();
    assert_eq!(json["group"], 3);
    unsafe { shadewire_compiled_free(compiled) };
    unsafe { shadewire_ir_free(ir) };
}

/// Without uniforms the texture takes the first binding, unless the uniform binding is reserved.
#[test]
fn reserve_uniform_binding_moves_the_textures_past_the_uniform_slot() {
    let document = r#"<materialx version="1.39">
  <image name="img" type="color3">
    <input name="file" type="filename" value="a.png" />
  </image>
  <surface_unlit name="SU" type="surfaceshader">
    <input name="emission_color" type="color3" nodename="img" />
  </surface_unlit>
  <surfacematerial name="M" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="SU" />
  </surfacematerial>
</materialx>"#;
    let mut mtlx = std::mem::MaybeUninit::<shadewire_mtlx_options>::uninit();
    unsafe { shadewire_mtlx_options_default(mtlx.as_mut_ptr()) };
    let mut mtlx = unsafe { mtlx.assume_init() };
    mtlx.values_as_uniforms = false;
    let ir = to_ir_with(document, &mtlx);
    let texture_binding = |reserve: bool| {
        let mut options = std::mem::MaybeUninit::<shadewire_compile_options>::uninit();
        unsafe { shadewire_compile_options_default(options.as_mut_ptr()) };
        let mut options = unsafe { options.assume_init() };
        options.first_binding = 10;
        options.reserve_uniform_binding = reserve;
        let compiled = compile(ir, &options);
        let json: serde_json::Value =
            serde_json::from_str(&text(unsafe { shadewire_compiled_reflection_json(compiled, null_mut()) })).unwrap();
        unsafe { shadewire_compiled_free(compiled) };
        assert!(json["uniform_binding"].is_null(), "{json}");
        json["textures"][0]["binding"].as_u64().unwrap()
    };
    assert_eq!((texture_binding(false), texture_binding(true)), (10, 11));
    unsafe { shadewire_ir_free(ir) };
}

#[test]
fn compiling_a_null_ir_is_an_invalid_argument() {
    let mut compiled = null_mut();
    let mut error = null_mut();
    let status = unsafe { shadewire_compile(null(), WGSL, null(), &mut compiled, &mut error) };
    assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
    assert!(compiled.is_null());
    assert_eq!(unsafe { shadewire_error_get_kind(error) }, shadewire_error_kind::SHADEWIRE_ERROR_INVALID_ARGUMENT);
    unsafe { shadewire_error_free(error) };
    unsafe { shadewire_compiled_free(null_mut()) };
}

fn sentinel<T>() -> *mut T {
    std::ptr::NonNull::<T>::dangling().as_ptr()
}

#[test]
fn error_is_untouched_on_success() {
    let example = default_example();
    let marker = sentinel::<shadewire_error>();
    let mut error: *mut shadewire_error = marker;
    let mut ir = null_mut();
    let status = unsafe {
        shadewire_mtlx_to_ir(example.as_ptr().cast(), example.len(), null(), null(), null(), &mut ir, &mut error)
    };
    assert_eq!((status, error), (shadewire_status::SHADEWIRE_OK, marker));
    let mut compiled = null_mut();
    assert_eq!(
        unsafe { shadewire_compile(ir, WGSL, null(), &mut compiled, &mut error) },
        shadewire_status::SHADEWIRE_OK
    );
    assert_eq!(error, marker);
    unsafe { shadewire_compiled_free(compiled) };
    unsafe { shadewire_ir_free(ir) };
}

#[test]
fn to_ir_failures_leave_out_untouched() {
    let marker = sentinel::<shadewire_ir>();
    let bad_utf8 = [0xffu8, 0xfe];
    let parse = "<materialx><nodegraph></materialx>";
    for (ptr, len) in [(bad_utf8.as_ptr().cast(), 2usize), (parse.as_ptr().cast(), parse.len())] {
        let mut out = marker;
        let mut error = null_mut();
        let status = unsafe { shadewire_mtlx_to_ir(ptr, len, null(), null(), null(), &mut out, &mut error) };
        assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
        assert_eq!(out, marker);
        assert!(!error.is_null());
        unsafe { shadewire_error_free(error) };
    }
}

#[test]
fn null_text_is_an_invalid_argument() {
    let mut ir = null_mut();
    let mut error = null_mut();
    let status = unsafe { shadewire_mtlx_to_ir(null(), 5, null(), null(), null(), &mut ir, &mut error) };
    assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
    assert!(ir.is_null());
    assert_eq!(unsafe { shadewire_error_get_kind(error) }, shadewire_error_kind::SHADEWIRE_ERROR_INVALID_ARGUMENT);
    unsafe { shadewire_error_free(error) };
}

#[test]
fn getters_on_null_return_null_with_zero_len() {
    let mut len = 99usize;
    unsafe {
        assert!(shadewire_ir_model(null()).is_null());
        assert!(shadewire_ir_dump(null(), &mut len).is_null());
        assert_eq!(len, 0);
        len = 99;
        assert!(shadewire_compiled_code(null(), &mut len).is_null());
        assert_eq!(len, 0);
        len = 99;
        assert!(shadewire_compiled_reflection_json(null(), &mut len).is_null());
        assert_eq!(len, 0);
        len = 99;
        assert!(shadewire_compiled_ir_dump(null(), &mut len).is_null());
        assert_eq!(len, 0);
        assert!(shadewire_compiled_model_struct(null()).is_null());
        assert!(shadewire_compiled_language(null()).is_null());
        assert!(shadewire_ir_source(null()).is_null());
        assert!(shadewire_ir_source_version(null()).is_null());
        assert!(shadewire_ir_structure_hash(null()).is_null());
        len = 99;
        assert!(shadewire_ir_json(null(), &mut len).is_null());
        assert_eq!(len, 0);
        assert_eq!(shadewire_strings_count(null()), 0);
        assert!(shadewire_strings_get(null(), 0).is_null());
        shadewire_strings_free(null_mut());
    }
}

fn compile_to(document: &str, kind: shadewire_target_kind, version: u32) -> Result<String, *mut shadewire_error> {
    let ir = to_ir(document);
    let mut compiled = null_mut();
    let mut error = null_mut();
    let target = shadewire_target { kind, version };
    let status = unsafe { shadewire_compile(ir, target, null(), &mut compiled, &mut error) };
    unsafe { shadewire_ir_free(ir) };
    if status != shadewire_status::SHADEWIRE_OK {
        return Err(error);
    }
    let code = text(unsafe { shadewire_compiled_code(compiled, null_mut()) });
    unsafe { shadewire_compiled_free(compiled) };
    Ok(code)
}

#[test]
fn every_target_compiles_the_default_example() {
    let example = default_example();
    let cases = [
        (SHADEWIRE_TARGET_WGSL, 0, "fn shadewire_eval_surface() -> StandardSurface"),
        (SHADEWIRE_TARGET_GLSL, 0, "#version 420 core"),
        (SHADEWIRE_TARGET_GLSL, 330, "#version 330 core"),
        (SHADEWIRE_TARGET_GLSL_ES, 0, "#version 300 es"),
        (SHADEWIRE_TARGET_GLSL_ES, 310, "#version 310 es"),
        (SHADEWIRE_TARGET_HLSL, 0, "StandardSurface shadewire_eval_surface() {"),
        (SHADEWIRE_TARGET_MSL, 0, "#include <metal_stdlib>"),
    ];
    for (kind, version, expect) in cases {
        let code = compile_to(&example, kind, version).unwrap_or_else(|_| panic!("kind {kind} {version}"));
        assert!(code.contains(expect), "kind {kind} {version}: {code}");
    }
}

#[test]
fn unknown_targets_and_versions_are_invalid_arguments() {
    let example = default_example();
    for (kind, version) in [(99, 0), (SHADEWIRE_TARGET_GLSL, 999), (SHADEWIRE_TARGET_GLSL_ES, 330)] {
        let error = compile_to(&example, kind, version).err().unwrap();
        assert_eq!(unsafe { shadewire_error_get_kind(error) }, shadewire_error_kind::SHADEWIRE_ERROR_INVALID_ARGUMENT);
        unsafe { shadewire_error_free(error) };
    }
    let ir = to_ir(&example);
    let marker = sentinel::<shadewire_compiled>();
    let mut out = marker;
    let mut error = null_mut();
    let target = shadewire_target { kind: 99, version: 0 };
    let status = unsafe { shadewire_compile(ir, target, null(), &mut out, &mut error) };
    assert_eq!((status, out), (shadewire_status::SHADEWIRE_ERROR, marker));
    unsafe { shadewire_error_free(error) };
    unsafe { shadewire_ir_free(ir) };
}

#[test]
fn the_ir_answers_what_python_s_ir_does() {
    let ir = to_ir(&default_example());
    assert_eq!(text(unsafe { shadewire_ir_model(ir) }), "standard_surface");
    assert_eq!(text(unsafe { shadewire_ir_source(ir) }), "mtlx");
    assert!(!text(unsafe { shadewire_ir_source_version(ir) }).is_empty());
    let hash = text(unsafe { shadewire_ir_structure_hash(ir) });
    assert!(!hash.is_empty());
    let mut len = 0usize;
    let json: serde_json::Value =
        serde_json::from_str(&text(unsafe { shadewire_ir_json(ir, &mut len) })).expect("the IR as JSON");
    assert!(len > 0);
    assert_eq!(json["version"], SHADEWIRE_IR_FORMAT_VERSION);
    assert_eq!(json["model"], "standard_surface");

    let mut optimized = null_mut();
    let mut error = null_mut();
    assert_eq!(unsafe { shadewire_ir_optimize(ir, &mut optimized, &mut error) }, shadewire_status::SHADEWIRE_OK);
    let before =
        serde_json::from_str::<serde_json::Value>(&text(unsafe { shadewire_ir_json(ir, null_mut()) })).unwrap();
    let after =
        serde_json::from_str::<serde_json::Value>(&text(unsafe { shadewire_ir_json(optimized, null_mut()) })).unwrap();
    let nodes = |doc: &serde_json::Value| doc["nodes"].as_array().map_or(0, Vec::len);
    assert!(nodes(&after) <= nodes(&before));
    assert_eq!(text(unsafe { shadewire_ir_model(optimized) }), "standard_surface");
    unsafe { shadewire_ir_free(optimized) };
    unsafe { shadewire_ir_free(ir) };
}

/// Hextiled lookups sample with explicit gradients: `grad` names two earlier vector2 nodes, before and after
/// optimizing, and a plain lookup has none.
#[test]
fn ir_json_samples_carry_their_gradients() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../vendor/materialx/resources/Materials/Examples/StandardSurface/standard_surface_onyx_hextiled.mtlx"
    );
    let ir = to_ir(&std::fs::read_to_string(path).unwrap());
    let mut optimized = null_mut();
    let mut error = null_mut();
    assert_eq!(unsafe { shadewire_ir_optimize(ir, &mut optimized, &mut error) }, shadewire_status::SHADEWIRE_OK);
    for handle in [ir, optimized] {
        let json: serde_json::Value =
            serde_json::from_str(&text(unsafe { shadewire_ir_json(handle, null_mut()) })).unwrap();
        let nodes = json["nodes"].as_array().unwrap();
        let samples: Vec<(usize, &serde_json::Value)> =
            nodes.iter().enumerate().filter(|(_, n)| n["kind"] == "sample").collect();
        assert!(!samples.is_empty());
        for (at, sample) in samples {
            let grad = sample["grad"].as_array().unwrap_or_else(|| panic!("sample {at} has no grad: {sample}"));
            assert_eq!(grad.len(), 2);
            for g in grad {
                let g = g.as_u64().unwrap() as usize;
                assert!(g < at, "grad {g} of sample {at} is not an earlier node");
                assert_eq!(nodes[g]["type"], "vector2");
            }
        }
    }
    unsafe { shadewire_ir_free(optimized) };
    unsafe { shadewire_ir_free(ir) };

    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../vendor/materialx/resources/Materials/Examples/StandardSurface/standard_surface_brass_tiled.mtlx"
    );
    let ir = to_ir(&std::fs::read_to_string(path).unwrap());
    let json: serde_json::Value = serde_json::from_str(&text(unsafe { shadewire_ir_json(ir, null_mut()) })).unwrap();
    let samples: Vec<&serde_json::Value> =
        json["nodes"].as_array().unwrap().iter().filter(|n| n["kind"] == "sample").collect();
    assert!(!samples.is_empty() && samples.iter().all(|s| s.get("grad").is_none()), "{samples:?}");
    unsafe { shadewire_ir_free(ir) };
}

#[test]
fn optimize_refuses_null() {
    let mut out = null_mut();
    let mut error = null_mut();
    assert_eq!(unsafe { shadewire_ir_optimize(null(), &mut out, &mut error) }, shadewire_status::SHADEWIRE_ERROR);
    assert_eq!(unsafe { shadewire_error_get_kind(error) }, shadewire_error_kind::SHADEWIRE_ERROR_INVALID_ARGUMENT);
    assert!(out.is_null());
    unsafe { shadewire_error_free(error) };
}

#[test]
fn material_names_lists_the_document_s_materials() {
    let doc = r#"<?xml version="1.0"?>
<materialx version="1.39">
  <standard_surface name="SR_a" type="surfaceshader" />
  <surfacematerial name="First" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="SR_a" />
  </surfacematerial>
  <surfacematerial name="Second" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="SR_a" />
  </surfacematerial>
</materialx>"#;
    let mut names = null_mut();
    let mut error = null_mut();
    let status = unsafe {
        shadewire_mtlx_material_names(doc.as_ptr().cast(), doc.len(), null(), null(), &mut names, &mut error)
    };
    assert_eq!(status, shadewire_status::SHADEWIRE_OK);
    let count = unsafe { shadewire_strings_count(names) };
    let listed: Vec<String> = (0..count).map(|i| text(unsafe { shadewire_strings_get(names, i) })).collect();
    assert_eq!(listed, ["First", "Second"]);
    assert!(unsafe { shadewire_strings_get(names, count) }.is_null());
    unsafe { shadewire_strings_free(names) };

    // Two materials and none named: the same error Python raises.
    let mut ir = null_mut();
    let status =
        unsafe { shadewire_mtlx_to_ir(doc.as_ptr().cast(), doc.len(), null(), null(), null(), &mut ir, &mut error) };
    assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
    assert_eq!(unsafe { shadewire_error_get_kind(error) }, shadewire_error_kind::SHADEWIRE_ERROR_MATERIAL);
    unsafe { shadewire_error_free(error) };
}

#[test]
fn implicit_derivatives_off_compiles_for_compute() {
    let ir = to_ir(&default_example());
    let mut options = std::mem::MaybeUninit::uninit();
    unsafe { shadewire_compile_options_default(options.as_mut_ptr()) };
    let mut options = unsafe { options.assume_init() };
    assert!(options.implicit_derivatives);
    options.implicit_derivatives = false;
    let compiled = compile(ir, &options);
    assert!(!text(unsafe { shadewire_compiled_code(compiled, null_mut()) }).is_empty());
    unsafe { shadewire_compiled_free(compiled) };
    unsafe { shadewire_ir_free(ir) };
}

/// Serves `mem://lib/a.mtlx` (which includes `b.mtlx`, a material) and fails anything else; counts its calls.
unsafe extern "C" fn mem_resolve(
    user_data: *mut std::ffi::c_void,
    base: *const std::os::raw::c_char,
    href: *const std::os::raw::c_char,
    out: *mut shadewire_resolution,
) {
    unsafe { *user_data.cast::<u32>() += 1 };
    let (base, href) = (text(base), text(href));
    let dir = base.rsplit_once('/').map_or("", |(dir, _)| dir);
    let location = CString::new(format!("{dir}/{href}")).unwrap();
    let body = match location.to_str().unwrap() {
        "mem://lib/a.mtlx" => r#"<materialx version="1.39"><xi:include href="b.mtlx"/></materialx>"#,
        "mem://lib/b.mtlx" => {
            r#"<materialx version="1.39">
  <standard_surface name="SR_b" type="surfaceshader" />
  <surfacematerial name="FromB" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="SR_b" />
  </surfacematerial>
</materialx>"#
        }
        "mem://lib/silent.mtlx" => return,
        _ => {
            let message = CString::new(format!("no {}", location.to_str().unwrap())).unwrap();
            unsafe { shadewire_resolution_fail(out, message.as_ptr()) };
            return;
        }
    };
    unsafe { shadewire_resolution_set(out, location.as_ptr(), body.as_ptr().cast(), body.len()) };
}

fn mem_options(calls: &mut u32) -> shadewire_mtlx_options {
    let mut options = std::mem::MaybeUninit::uninit();
    unsafe { shadewire_mtlx_options_default(options.as_mut_ptr()) };
    let mut options = unsafe { options.assume_init() };
    assert!(options.resolve.is_none());
    options.resolve = Some(mem_resolve);
    options.resolve_user_data = (calls as *mut u32).cast();
    options
}

fn lower_at(doc: &str, options: &shadewire_mtlx_options) -> Result<*mut shadewire_ir, String> {
    let location = CString::new("mem://lib/top.mtlx").unwrap();
    let (mut ir, mut error) = (null_mut(), null_mut());
    let status = unsafe {
        shadewire_mtlx_to_ir(doc.as_ptr().cast(), doc.len(), location.as_ptr(), null(), options, &mut ir, &mut error)
    };
    if status == shadewire_status::SHADEWIRE_OK {
        return Ok(ir);
    }
    let message = text(unsafe { shadewire_error_message(error, null_mut()) });
    unsafe { shadewire_error_free(error) };
    Err(message)
}

#[test]
fn a_host_resolver_serves_nested_includes() {
    let mut calls = 0;
    let options = mem_options(&mut calls);
    let doc = r#"<materialx version="1.39"><xi:include href="a.mtlx"/></materialx>"#;
    let location = CString::new("mem://lib/top.mtlx").unwrap();
    let (mut names, mut error) = (null_mut(), null_mut());
    let status = unsafe {
        shadewire_mtlx_material_names(
            doc.as_ptr().cast(),
            doc.len(),
            location.as_ptr(),
            &options,
            &mut names,
            &mut error,
        )
    };
    assert_eq!(status, shadewire_status::SHADEWIRE_OK);
    assert_eq!(text(unsafe { shadewire_strings_get(names, 0) }), "FromB");
    unsafe { shadewire_strings_free(names) };

    let ir = lower_at(doc, &options).unwrap();
    assert_eq!(text(unsafe { shadewire_ir_model(ir) }), "standard_surface");
    unsafe { shadewire_ir_free(ir) };
    assert_eq!(calls, 4, "a.mtlx and b.mtlx, once per call");
}

#[test]
fn resolver_failures_become_errors() {
    let mut calls = 0;
    let options = mem_options(&mut calls);
    let err = lower_at(r#"<materialx><xi:include href="x.mtlx"/></materialx>"#, &options).unwrap_err();
    assert!(err.contains("no mem://lib/x.mtlx"), "{err}");
    let err = lower_at(r#"<materialx><xi:include href="silent.mtlx"/></materialx>"#, &options).unwrap_err();
    assert!(err.contains("no answer for silent.mtlx"), "{err}");
}

#[test]
fn list_dependencies_names_includes_and_prefixed_files() {
    let doc = r#"<materialx version="1.39" fileprefix="tex/">
  <xi:include href="lib/a.mtlx"/>
  <image name="i" type="color3"><input name="file" type="filename" value="base.png"/></image>
  <xi:include href="lib/a.mtlx"/>
</materialx>"#;
    let (mut deps, mut error) = (null_mut(), null_mut());
    let status =
        unsafe { shadewire_mtlx_list_dependencies(doc.as_ptr().cast(), doc.len(), null(), &mut deps, &mut error) };
    assert_eq!(status, shadewire_status::SHADEWIRE_OK);
    assert_eq!(unsafe { shadewire_dependencies_include_count(deps) }, 1);
    assert_eq!(text(unsafe { shadewire_dependencies_include(deps, 0) }), "lib/a.mtlx");
    assert!(unsafe { shadewire_dependencies_include(deps, 1) }.is_null());
    assert_eq!(unsafe { shadewire_dependencies_file_count(deps) }, 1);
    assert_eq!(text(unsafe { shadewire_dependencies_file(deps, 0) }), "tex/base.png");
    unsafe { shadewire_dependencies_free(deps) };
    assert_eq!(unsafe { shadewire_dependencies_file_count(null()) }, 0);
    unsafe { shadewire_dependencies_free(null_mut()) };

    let bad = "<materialx";
    let status =
        unsafe { shadewire_mtlx_list_dependencies(bad.as_ptr().cast(), bad.len(), null(), &mut deps, &mut error) };
    assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
    assert_eq!(unsafe { shadewire_error_get_kind(error) }, shadewire_error_kind::SHADEWIRE_ERROR_PARSE);
    unsafe { shadewire_error_free(error) };
}
