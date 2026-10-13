//! Reading documents and textures through Godot: `FileAccess` sees `res://` inside a `.pck`, `user://` and OS
//! paths alike.

use godot::classes::{FileAccess, Image, ImageTexture, ResourceLoader, Texture2D};
use godot::global::Error as GdError;
use godot::prelude::*;
use shadewire_mtlx::{Resolved, Resolver};

use crate::compile::{ImageFormat, image_format, reference};
use crate::location::is_url;

const NO_URLS: &str = "http(s) sources load with load_material_async only";

pub fn read_bytes(location: &str) -> Result<PackedByteArray, String> {
    if is_url(location) {
        return Err(format!("{location}: {NO_URLS}"));
    }
    let bytes = FileAccess::get_file_as_bytes(location);
    if bytes.is_empty() && FileAccess::get_open_error() != GdError::OK {
        return Err(format!("cannot read {location}"));
    }
    Ok(bytes)
}

pub fn read_text(location: &str) -> Result<String, String> {
    let bytes = read_bytes(location)?;
    String::from_utf8(bytes.to_vec()).map_err(|_| format!("{location} is not UTF-8"))
}

/// Includes joined to the including document's location and read with `FileAccess`.
pub fn resolver() -> Resolver {
    Resolver::new(|base, href| {
        let location = reference(Some(base), href)?;
        let text = read_text(&location)?;
        Ok(Resolved { location, text })
    })
}

/// A texture for the shader: an imported project texture through `ResourceLoader` (exports contain the
/// imported texture, not the source file), anything else decoded from its bytes (PNG, JPEG or WebP).
pub fn load_texture(location: &str) -> Result<Gd<Texture2D>, String> {
    let mut loader = ResourceLoader::singleton();
    if location.starts_with("res://") && loader.exists_ex(location).type_hint("Texture2D").done() {
        return loader
            .load_ex(location)
            .type_hint("Texture2D")
            .done()
            .and_then(|r| r.try_cast::<Texture2D>().ok())
            .ok_or_else(|| format!("cannot load {location}"));
    }
    texture_from_bytes(location, &read_bytes(location)?)
}

/// A texture decoded from the bytes of the file at `location` (PNG, JPEG or WebP), with mipmaps.
pub fn texture_from_bytes(location: &str, bytes: &PackedByteArray) -> Result<Gd<Texture2D>, String> {
    let mut image = Image::new_gd();
    let status = match image_format(bytes.as_slice()) {
        Some(ImageFormat::Png) => image.load_png_from_buffer(bytes),
        Some(ImageFormat::Jpeg) => image.load_jpg_from_buffer(bytes),
        Some(ImageFormat::Webp) => image.load_webp_from_buffer(bytes),
        None => return Err(format!("{location}: unsupported runtime format (PNG, JPEG and WebP load at runtime)")),
    };
    if status != GdError::OK {
        return Err(format!("cannot decode {location}"));
    }
    image.generate_mipmaps();
    ImageTexture::create_from_image(&image).map(|t| t.upcast()).ok_or_else(|| format!("cannot upload {location}"))
}
