//! The committed header matches the crate. Bless with SHADEWIRE_BLESS_HEADER=1.

use std::path::Path;

#[test]
fn the_committed_header_is_up_to_date() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config = cbindgen::Config::from_file(crate_dir.join("cbindgen.toml")).unwrap();
    let bindings = cbindgen::Builder::new().with_crate(crate_dir).with_config(config).generate().unwrap();
    let mut generated = Vec::new();
    bindings.write(&mut generated);
    let generated = String::from_utf8(generated).unwrap().replace("\r\n", "\n");
    let path = crate_dir.join("include/shadewire.h");
    if std::env::var("SHADEWIRE_BLESS_HEADER").as_deref() == Ok("1") {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &generated).unwrap();
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
    assert!(committed == generated, "include/shadewire.h is stale: rerun with SHADEWIRE_BLESS_HEADER=1");
}
