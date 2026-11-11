//! Runs the C tests of the MDL modules handle (tests/mdl_modules.c).

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use shadewire_capi_ctest::{shadewire_ctest_mdl_modules_lower_many, shadewire_ctest_mdl_modules_old_options};

fn report(step: i32, report: &[c_char]) {
    let message = unsafe { CStr::from_ptr(report.as_ptr()) }.to_string_lossy().into_owned();
    assert_eq!(step, 0, "step {step}: {message}");
}

#[test]
fn mdl_modules_lower_many() {
    let dir = std::env::temp_dir().join(format!("shadewire-ctest-mdl-modules-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("tinted.mdl");
    let text = "mdl 1.7;\nimport ::df::*;\nexport material tinted(color tint = color(0.5)) = material(surface: \
                material_surface(scattering: df::diffuse_reflection_bsdf(tint: tint)));\n";
    std::fs::write(&file, text).unwrap();
    let path = CString::new(file.to_str().unwrap()).unwrap();
    let mut buffer = vec![0 as c_char; 512];
    let step = unsafe { shadewire_ctest_mdl_modules_lower_many(path.as_ptr(), buffer.as_mut_ptr(), buffer.len()) };
    std::fs::remove_dir_all(&dir).unwrap();
    report(step, &buffer);
}

#[test]
fn mdl_modules_old_options() {
    let mut buffer = vec![0 as c_char; 512];
    let step = unsafe { shadewire_ctest_mdl_modules_old_options(buffer.as_mut_ptr(), buffer.len()) };
    report(step, &buffer);
}
