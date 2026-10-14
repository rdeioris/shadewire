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
    let ir = to_ir(text, location, options.material.as_deref(), &options.ir(), &resolver).map_err(|e| e.to_string())?;
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

/// The width and height an image's header declares (PNG, JPEG or WebP), read without decoding: a small file can
/// declare an image of gigabytes. `None` when the header is not one of these or is cut short.
pub fn image_size(bytes: &[u8]) -> Option<(u64, u64)> {
    let be = |i: usize, n: usize| Some(bytes.get(i..i + n)?.iter().fold(0u64, |v, b| v << 8 | *b as u64));
    let le = |i: usize, n: usize| Some(bytes.get(i..i + n)?.iter().rev().fold(0u64, |v, b| v << 8 | *b as u64));
    match image_format(bytes)? {
        ImageFormat::Png if bytes.get(12..16)? == b"IHDR" => Some((be(16, 4)?, be(20, 4)?)),
        ImageFormat::Png => None,
        ImageFormat::Jpeg => {
            // Segments up to the first start-of-frame marker (C0 to CF, but C4, C8 and CC are not frames).
            let mut i = 2;
            loop {
                if *bytes.get(i)? != 0xff {
                    return None;
                }
                let marker = *bytes.get(i + 1)?;
                match marker {
                    0xff => i += 1,
                    0x01 | 0xd0..=0xd7 => i += 2,
                    0xc0..=0xcf if ![0xc4, 0xc8, 0xcc].contains(&marker) => {
                        return Some((be(i + 7, 2)?, be(i + 5, 2)?));
                    }
                    _ => i += 2 + be(i + 2, 2)? as usize,
                }
            }
        }
        ImageFormat::Webp => match bytes.get(12..16)? {
            b"VP8 " if bytes.get(23..26)? == [0x9d, 0x01, 0x2a] => Some((le(26, 2)? & 0x3fff, le(28, 2)? & 0x3fff)),
            b"VP8L" if *bytes.get(20)? == 0x2f => {
                let bits = le(21, 4)?;
                Some(((bits & 0x3fff) + 1, (bits >> 14 & 0x3fff) + 1))
            }
            b"VP8X" => Some((le(24, 3)? + 1, le(27, 3)? + 1)),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::Limits;
    use shadewire_mtlx::Resolved;

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

    #[test]
    fn image_headers_give_the_declared_size() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend([0, 0, 0x40, 0, 0, 0, 0x20, 0]);
        assert_eq!(image_size(&png), Some((16384, 8192)));
        assert_eq!(image_size(&png[..20]), None);
        // SOI, an APP0 segment, a DHT (C4, not a frame), fill bytes, then a progressive frame (C2): height, width.
        let jpeg = [0xff, 0xd8, 0xff, 0xe0, 0, 4, 0, 0, 0xff, 0xc4, 0, 2, 0xff, 0xff, 0xc2, 0, 11, 8, 0x10, 0, 0x30, 0];
        assert_eq!(image_size(&jpeg), Some((0x3000, 0x1000)));
        assert_eq!(image_size(&jpeg[..jpeg.len() - 1]), None);
        let webp = |chunk: &[u8], rest: &[u8]| [&b"RIFF\0\0\0\0WEBP"[..], chunk, b"\0\0\0\0", rest].concat();
        let vp8 = webp(b"VP8 ", &[0, 0, 0, 0x9d, 0x01, 0x2a, 0x00, 0x04, 0x00, 0x02]);
        assert_eq!(image_size(&vp8), Some((1024, 512)));
        // 14 bits each, minus one: 4096 and 2048.
        let bits: u32 = 4095 | 2047 << 14;
        let vp8l = webp(b"VP8L", &[&[0x2f][..], &bits.to_le_bytes()].concat());
        assert_eq!(image_size(&vp8l), Some((4096, 2048)));
        let vp8x = webp(b"VP8X", &[0, 0, 0, 0, 0xff, 0xff, 0x00, 0x0f, 0, 0]);
        assert_eq!(image_size(&vp8x), Some((65536, 16)));
        assert_eq!(image_size(b"#?RADIANCE\n"), None);
    }

    /// A material sampling `count` textures, each `t<i>.png`.
    fn textures(count: usize) -> String {
        let mut text =
            String::from(r#"<materialx version="1.39"><nodegraph name="NG"><constant name="c0" type="float"/>"#);
        for i in 0..count {
            text.push_str(&format!(
                r#"<image name="i{i}" type="float"><input name="file" type="filename" value="t{i}.png"/></image>
                <add name="c{}" type="float"><input name="in1" type="float" nodename="c{i}"/><input name="in2" type="float" nodename="i{i}"/></add>"#,
                i + 1
            ));
        }
        text.push_str(&format!(
            r#"<output name="out" type="float" nodename="c{count}"/></nodegraph>
            <standard_surface name="SR" type="surfaceshader"><input name="base" type="float" nodegraph="NG" output="out"/></standard_surface>
            <surfacematerial name="M" type="material"><input name="surfaceshader" type="surfaceshader" nodename="SR"/></surfacematerial>
            </materialx>"#
        ));
        text
    }

    #[test]
    fn runtime_loads_are_limited_by_default() {
        let options = LoadOptions::default();
        assert_eq!(compile(&textures(16), None, &options, Resolver::default()).unwrap().reflection.textures.len(), 16);
        let err = compile(&textures(17), None, &options, Resolver::default()).unwrap_err();
        assert_eq!(err, "the material samples 17 textures, more than 16 (max_textures)");
        let options = LoadOptions { limits: Limits { max_nodes: Some(40), ..Limits::NONE }, ..LoadOptions::default() };
        let err = compile(&textures(17), None, &options, Resolver::default()).unwrap_err();
        assert_eq!(err, "the material needs more than 40 nodes (max_nodes)");
        let unlimited = LoadOptions { limits: Limits::NONE, ..LoadOptions::default() };
        assert!(compile(&textures(17), None, &unlimited, Resolver::default()).is_ok());
    }

    #[test]
    fn fractal_octaves_are_clamped_in_the_shader() {
        let text = r#"<materialx version="1.39">
          <fractal3d name="f" type="float"><input name="octaves" type="integer" value="1000000"/></fractal3d>
          <standard_surface name="SR" type="surfaceshader"><input name="base" type="float" nodename="f"/></standard_surface>
          <surfacematerial name="M" type="material"><input name="surfaceshader" type="surfaceshader" nodename="SR"/></surfacematerial>
        </materialx>"#;
        let options = LoadOptions { optimize: false, ..LoadOptions::default() };
        let out = compile(text, None, &options, Resolver::default()).unwrap();
        assert!(out.code.contains("i < min(octaves, 16);"), "{}", out.code);
    }

    #[test]
    fn an_included_file_is_never_quoted() {
        let resolver = Resolver::new(|_, href| {
            Ok(Resolved { location: format!("user://{href}"), text: "<keys><key value=\"s3cr3t\"/></keys>".into() })
        });
        let text = r#"<materialx version="1.39"><xi:include href="../../secret"/></materialx>"#;
        let err = compile(text, Some("user://mods/m.mtlx"), &LoadOptions::default(), resolver).unwrap_err();
        assert_eq!(err, "user://../../secret:1 (): parse error: not a MaterialX document");
    }
}
