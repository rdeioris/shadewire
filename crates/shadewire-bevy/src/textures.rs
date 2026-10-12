//! The material's textures: texture k of the reflection goes to extension slot k (bindings `101 + 2k`
//! and `102 + 2k`), read from disk and uploaded raw. The generated shader converts each sample from the
//! file's colorspace itself, so the image is never sRGB (a `*_srgb` view would decode it twice).
//!
//! Paths are read as the document gives them: an absolute path is opened as is, and a relative one
//! (after the document's `fileprefix`) is joined to the `.mtlx` file's directory, `..` included. Nothing
//! confines them to that directory, so an app loading `.mtlx` files it does not trust should vet the
//! reflection's texture paths (or the document) before building the material.

use std::path::{Path, PathBuf};

use bevy::asset::{Assets, Handle, RenderAssetUsages};
use bevy::image::{
    CompressedImageFormats, Image, ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor, ImageType,
};
use shadewire::reflection::TextureReflection;
use shadewire::{AddressMode, FilterMode, Reflection};

use crate::error::ShadewireError;
use crate::extension::{FIRST_BINDING, TEXTURE_SLOTS};

/// The handles for the extension's slots `t0..t7`; slots past the material's textures stay `None`.
pub type Slots = [Option<Handle<Image>>; TEXTURE_SLOTS];

/// The reflection's textures in slot order, refused when there are more than the extension holds or
/// when texture k is not at the extension's bindings `101 + 2k` and `102 + 2k`.
pub fn slots(reflection: &Reflection) -> Result<&[TextureReflection], ShadewireError> {
    let count = reflection.textures.len();
    if count > TEXTURE_SLOTS {
        return Err(ShadewireError::TooManyTextures { count, limit: TEXTURE_SLOTS });
    }
    for (k, texture) in reflection.textures.iter().enumerate() {
        let binding = FIRST_BINDING + 1 + 2 * k as u32;
        if texture.binding != binding || texture.sampler_binding != binding + 1 {
            return Err(ShadewireError::Bindings(format!(
                "texture {k} ({}) is at bindings {} and {}, the extension's slot {k} at {} and {}",
                texture.file,
                texture.binding,
                texture.sampler_binding,
                binding,
                binding + 1
            )));
        }
    }
    Ok(&reflection.textures)
}

/// Bevy's address mode for a MaterialX one. `constant` clamps: the generated shader already returns
/// the default value outside [0, 1].
pub fn address_mode(mode: AddressMode) -> ImageAddressMode {
    match mode {
        AddressMode::Constant => ImageAddressMode::ClampToEdge,
        AddressMode::Clamp => ImageAddressMode::ClampToEdge,
        AddressMode::Periodic => ImageAddressMode::Repeat,
        AddressMode::Mirror => ImageAddressMode::MirrorRepeat,
    }
}

/// Bevy's filter for a MaterialX one. `cubic` is filtered linearly (the reflection notes the approximation).
pub fn filter_mode(mode: FilterMode) -> ImageFilterMode {
    match mode {
        FilterMode::Closest => ImageFilterMode::Nearest,
        FilterMode::Linear => ImageFilterMode::Linear,
        FilterMode::Cubic => ImageFilterMode::Linear,
    }
}

/// The sampler of one texture: its u and v address modes and its filter for magnification,
/// minification and mipmaps.
pub fn sampler(texture: &TextureReflection) -> ImageSampler {
    let filter = filter_mode(texture.filter);
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        label: Some(format!("shadewire texture {}", texture.file)),
        address_mode_u: address_mode(texture.address[0]),
        address_mode_v: address_mode(texture.address[1]),
        mag_filter: filter,
        min_filter: filter,
        mipmap_filter: filter,
        ..ImageSamplerDescriptor::default()
    })
}

/// The file of `texture`, relative to `dir` (the `.mtlx` file's directory) unless it is absolute.
pub fn file_path(texture: &TextureReflection, dir: &Path) -> PathBuf {
    dir.join(&texture.file)
}

/// Reads and decodes one texture as a raw (non-sRGB) image with its reflection sampler.
pub fn decode(texture: &TextureReflection, dir: &Path) -> Result<Image, ShadewireError> {
    let path = file_path(texture, dir);
    let bytes = std::fs::read(&path).map_err(|source| ShadewireError::TextureRead { path: path.clone(), source })?;
    let Some(extension) = path.extension().and_then(|e| e.to_str()) else {
        return Err(ShadewireError::TextureDecode { path, message: "no file extension".to_string() });
    };
    let extension = extension.to_ascii_lowercase();
    let image = Image::from_buffer(
        &bytes,
        ImageType::Extension(&extension),
        CompressedImageFormats::NONE,
        false,
        sampler(texture),
        RenderAssetUsages::RENDER_WORLD,
    );
    image.map_err(|e| ShadewireError::TextureDecode { path, message: e.to_string() })
}

/// Decodes every texture of the reflection, then adds them to `images` in slot order. Nothing is added
/// when one of them fails.
pub fn load(reflection: &Reflection, dir: &Path, images: &mut Assets<Image>) -> Result<Slots, ShadewireError> {
    let mut decoded = Vec::new();
    for texture in slots(reflection)? {
        decoded.push(decode(texture, dir)?);
    }
    let mut handles: Slots = Default::default();
    for (slot, image) in decoded.into_iter().enumerate() {
        handles[slot] = Some(images.add(image));
    }
    Ok(handles)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texture(file: &str, address: [AddressMode; 2], filter: FilterMode) -> TextureReflection {
        TextureReflection {
            index: 0,
            binding: 101,
            sampler_binding: 102,
            file: file.to_string(),
            colorspace: None,
            address,
            filter,
        }
    }

    fn reflection(files: &[&str]) -> Reflection {
        let mut r: Reflection = empty_reflection();
        r.textures = files
            .iter()
            .enumerate()
            .map(|(k, f)| {
                let mut t = texture(f, [AddressMode::Periodic, AddressMode::Periodic], FilterMode::Linear);
                t.index = k;
                t.binding = FIRST_BINDING + 1 + 2 * k as u32;
                t.sampler_binding = FIRST_BINDING + 2 + 2 * k as u32;
                t
            })
            .collect();
        r
    }

    /// An empty reflection (one uniform-less DAG), whose texture list the tests fill.
    fn empty_reflection() -> Reflection {
        use shadewire::{BindingOptions, Dag, SourceInfo, reflect};
        let dag = Dag::new("surface_unlit");
        let info = SourceInfo { source: "t".into(), mtlx_version: "1.39".into() };
        let options = BindingOptions { group: 3, first_binding: 100, reserve_uniform_binding: true };
        reflect(&dag, &options, &info).unwrap()
    }

    fn png(dir: &Path, name: &str) {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&[255, 128, 0, 255]).unwrap();
        std::fs::write(dir.join(name), bytes).unwrap();
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("shadewire-bevy-textures-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn slots_follow_reflection_order() {
        let r = reflection(&["a.png", "b.png", "c.png"]);
        let files: Vec<&str> = slots(&r).unwrap().iter().map(|t| t.file.as_str()).collect();
        assert_eq!(files, ["a.png", "b.png", "c.png"]);
    }

    #[test]
    fn nine_textures_are_refused() {
        let names: Vec<String> = (0..9).map(|i| format!("t{i}.png")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let err = slots(&reflection(&refs)).unwrap_err();
        assert!(matches!(err, ShadewireError::TooManyTextures { count: 9, limit: 8 }), "{err}");
        let eight = reflection(&refs[..8]);
        assert_eq!(slots(&eight).unwrap().len(), 8);
    }

    #[test]
    fn textures_off_their_slot_bindings_are_refused() {
        let mut r = reflection(&["a.png", "b.png"]);
        r.textures[1].binding = 105;
        let err = slots(&r).unwrap_err();
        assert!(matches!(err, ShadewireError::Bindings(_)), "{err}");
        assert!(err.to_string().contains("b.png"), "{err}");
        let mut r = reflection(&["a.png"]);
        r.textures[0].sampler_binding = 101;
        assert!(matches!(slots(&r).unwrap_err(), ShadewireError::Bindings(_)));
    }

    #[test]
    fn sampler_maps_address_and_filter() {
        let t = texture("x.png", [AddressMode::Mirror, AddressMode::Constant], FilterMode::Closest);
        let ImageSampler::Descriptor(d) = sampler(&t) else { panic!("default sampler") };
        assert_eq!(
            (d.address_mode_u, d.address_mode_v),
            (ImageAddressMode::MirrorRepeat, ImageAddressMode::ClampToEdge)
        );
        assert_eq!(
            (d.mag_filter, d.min_filter, d.mipmap_filter),
            (ImageFilterMode::Nearest, ImageFilterMode::Nearest, ImageFilterMode::Nearest)
        );
        assert_eq!(address_mode(AddressMode::Periodic), ImageAddressMode::Repeat);
        assert_eq!(address_mode(AddressMode::Clamp), ImageAddressMode::ClampToEdge);
        assert_eq!(filter_mode(FilterMode::Linear), ImageFilterMode::Linear);
        assert_eq!(filter_mode(FilterMode::Cubic), ImageFilterMode::Linear);
    }

    #[test]
    fn files_resolve_against_the_mtlx_directory() {
        let dir = scratch("load");
        std::fs::create_dir_all(dir.join("img")).unwrap();
        png(&dir.join("img"), "a.png");
        png(&dir, "b.png");
        let r = reflection(&["img/a.png", "b.png"]);
        let mut images = Assets::<Image>::default();
        let handles = load(&r, &dir, &mut images).unwrap();
        assert!(handles[0].is_some() && handles[1].is_some() && handles[2..].iter().all(Option::is_none));
        let image = images.get(handles[0].as_ref().unwrap()).unwrap();
        assert!(!image.texture_descriptor.format.is_srgb(), "{:?}", image.texture_descriptor.format);
        assert_eq!(images.len(), 2);
    }

    #[test]
    fn a_missing_file_is_named_and_nothing_is_added() {
        let dir = scratch("missing");
        png(&dir, "here.png");
        let r = reflection(&["here.png", "gone.png"]);
        let mut images = Assets::<Image>::default();
        let err = load(&r, &dir, &mut images).unwrap_err();
        assert!(err.to_string().contains("gone.png"), "{err}");
        assert_eq!(images.len(), 0);
    }

    #[test]
    fn an_undecodable_file_is_named() {
        let dir = scratch("broken");
        std::fs::write(dir.join("broken.png"), b"not a png").unwrap();
        let err = decode(&reflection(&["broken.png"]).textures[0], &dir).unwrap_err();
        assert!(matches!(err, ShadewireError::TextureDecode { .. }), "{err}");
        assert!(err.to_string().contains("broken.png"), "{err}");
    }
}
