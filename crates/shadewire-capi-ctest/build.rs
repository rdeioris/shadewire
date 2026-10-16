//! Compiles the C tests (the smoke test, the graph builder tests) against the committed header, warnings as errors.

fn main() {
    println!("cargo:rerun-if-changed=tests/smoke.c");
    println!("cargo:rerun-if-changed=tests/builder.c");
    println!("cargo:rerun-if-changed=../shadewire-capi/include/shadewire.h");
    let mut build = cc::Build::new();
    build.file("tests/smoke.c").file("tests/builder.c").include("../shadewire-capi/include").warnings_into_errors(true);
    // MSVC has no C99 mode (its oldest is C11); every other compiler builds the file as strict C99.
    let msvc = build.get_compiler().is_like_msvc();
    build.std(if msvc { "c11" } else { "c99" });
    // The test reads its input with plain fopen, which the Windows CRT deprecates in favour of fopen_s.
    build.define("_CRT_SECURE_NO_WARNINGS", None);
    build.compile("shadewire_smoke");
}
