//! Reading documents and textures through Godot: `FileAccess` sees `res://` inside a `.pck`, `user://` and OS
//! paths alike.

use std::sync::Arc;

use godot::classes::file_access::ModeFlags;
use godot::classes::{FileAccess, Image, ImageTexture, ResourceLoader, Texture2D};
use godot::global::Error as GdError;
use godot::prelude::*;
use shadewire_mtlx::{Resolved, Resolver};

use crate::compile::{ImageFormat, image_format, image_size, reference};
use crate::location::is_url;
use crate::options::{Budget, check_pixels};

const NO_URLS: &str = "http(s) sources load with load_material_async only";

/// The file at `location`, charged to `budget` by its length before it is read.
pub fn read_bytes(location: &str, budget: &Budget) -> Result<PackedByteArray, String> {
    if is_url(location) {
        return Err(format!("{location}: {NO_URLS}"));
    }
    let cannot = || format!("cannot read {location}");
    let mut file = FileAccess::open(location, ModeFlags::READ).ok_or_else(cannot)?;
    let length = file.get_length();
    budget.charge(location, length)?;
    let bytes = file.get_buffer(i64::try_from(length).map_err(|_| cannot())?);
    if (bytes.len() as u64) < length {
        return Err(cannot());
    }
    Ok(bytes)
}

pub fn read_text(location: &str, budget: &Budget) -> Result<String, String> {
    let bytes = read_bytes(location, budget)?;
    String::from_utf8(bytes.to_vec()).map_err(|_| format!("{location} is not UTF-8"))
}

/// Includes joined to the including document's location and read with `FileAccess`, charged to `budget`.
pub fn resolver(budget: Arc<Budget>) -> Resolver {
    Resolver::new(move |base, href| {
        let location = reference(Some(base), href)?;
        let text = read_text(&location, &budget)?;
        Ok(Resolved { location, text })
    })
}

/// A texture for the shader: an imported project texture through `ResourceLoader` (exports contain the
/// imported texture, not the source file), anything else decoded from its bytes (PNG, JPEG or WebP). Files read are
/// charged to `budget`; either kind is refused over `max_pixels`.
pub fn load_texture(location: &str, budget: &Budget, max_pixels: Option<u64>) -> Result<Gd<Texture2D>, String> {
    let mut loader = ResourceLoader::singleton();
    if location.starts_with("res://") && loader.exists_ex(location).type_hint("Texture2D").done() {
        let texture = loader
            .load_ex(location)
            .type_hint("Texture2D")
            .done()
            .and_then(|r| r.try_cast::<Texture2D>().ok())
            .ok_or_else(|| format!("cannot load {location}"))?;
        check_pixels(location, texture.get_width() as u64, texture.get_height() as u64, max_pixels)?;
        return Ok(texture);
    }
    texture_from_bytes(location, &read_bytes(location, budget)?, max_pixels)
}

/// A texture decoded from the bytes of the file at `location` (PNG, JPEG or WebP), with mipmaps. The size its header
/// declares is checked against `max_pixels` before decoding, and the decoded image's after.
pub fn texture_from_bytes(
    location: &str,
    bytes: &PackedByteArray,
    max_pixels: Option<u64>,
) -> Result<Gd<Texture2D>, String> {
    if let Some((width, height)) = image_size(bytes.as_slice()) {
        check_pixels(location, width, height, max_pixels)?;
    }
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
    check_pixels(location, image.get_width() as u64, image.get_height() as u64, max_pixels)?;
    image.generate_mipmaps();
    ImageTexture::create_from_image(&image).map(|t| t.upcast()).ok_or_else(|| format!("cannot upload {location}"))
}
