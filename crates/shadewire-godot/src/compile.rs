//! The compile core every entry point shares, free of Godot calls: MaterialX text to the `.gdshader` and its
//! reflection, and where each texture lives.

use shadewire_gdshader::GdshaderOutput;
use shadewire_mtlx::{Resolver, to_ir};

use crate::location::{is_absolute, is_url, join};
use crate::options::LoadOptions;

/// `to_ir` then the Godot back end. `location` names the document; `resolver` reads its includes.
pub fn compile(
    text: &str,
    location: Option<&str>,
    options: &LoadOptions,
    resolver: Resolver,
) -> Result<GdshaderOutput, String> {
    let ir = to_ir(text, location, options.material.as_deref(), &options.mtlx(resolver)).map_err(|e| e.to_string())?;
    shadewire_gdshader::compile(&ir.dag, &ir.source, &options.gdshader()).map_err(|e| e.to_string())
}

/// Where `href`, named by the document at `document`, lives. A remote document may not reach local files, and a
/// relative reference needs a document location.
pub fn reference(document: Option<&str>, href: &str) -> Result<String, String> {
    let location = match document {
        Some(document) => join(document, href),
        None if is_absolute(href) => join("", href),
        None => return Err(format!("{href}: a relative reference needs a document location")),
    };
    if document.is_some_and(is_url) && !is_url(&location) {
        return Err(format!("{href}: a remote document cannot reference local files"));
    }
    Ok(location)
}

/// The image formats runtime loading decodes, told by their magic bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
}

pub fn image_format(bytes: &[u8]) -> Option<ImageFormat> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(ImageFormat::Png)
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(ImageFormat::Jpeg)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(ImageFormat::Webp)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../vendor/materialx/resources/Materials/Examples");

    #[test]
    fn a_vendored_example_compiles_with_its_textures_located() {
        let location = format!("{EXAMPLES}/StandardSurface/standard_surface_brass_tiled.mtlx");
        let text = std::fs::read_to_string(&location).unwrap();
        let out = compile(&text, Some(&location), &LoadOptions::default(), Resolver::default()).unwrap();
        assert!(out.code.starts_with("shader_type spatial;"), "{}", out.code);
        assert_eq!(out.uniform_names.len(), out.reflection.uniforms.len());
        // The document's fileprefix climbs to resources/Images (only grid.png is vendored there).
        let path = reference(Some(&location), &out.reflection.textures[0].file).unwrap();
        assert!(path.contains("/vendor/materialx/resources/Images/brass_") && !path.contains(".."), "{path}");
    }

    #[test]
    fn references_follow_the_remote_rules() {
        assert_eq!(reference(Some("res://m/a.mtlx"), "t.png").unwrap(), "res://m/t.png");
        assert_eq!(reference(None, "user://t.png").unwrap(), "user://t.png");
        assert!(reference(None, "t.png").unwrap_err().contains("needs a document location"));
        assert!(reference(Some("https://x.org/a.mtlx"), "C:/secret.png").unwrap_err().contains("remote document"));
        assert_eq!(reference(Some("https://x.org/a.mtlx"), "t.png").unwrap(), "https://x.org/t.png");
    }

    #[test]
    fn images_are_told_by_their_magic_bytes() {
        assert_eq!(image_format(b"\x89PNG\r\n\x1a\n...."), Some(ImageFormat::Png));
        assert_eq!(image_format(&[0xff, 0xd8, 0xff, 0xe0]), Some(ImageFormat::Jpeg));
        assert_eq!(image_format(b"RIFF\0\0\0\0WEBPVP8 "), Some(ImageFormat::Webp));
        assert_eq!(image_format(b"#?RADIANCE\n"), None);
    }
}
