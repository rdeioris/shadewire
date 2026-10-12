//! Translated shader output: the opaque `shadewire_blob` handle.

/// A translated shader, text or binary. Free it with `shadewire_blob_free`.
///
/// Text blobs are followed by a NUL byte that the reported length does not count.
pub struct shadewire_blob {
    bytes: Vec<u8>,
    is_text: bool,
}

impl shadewire_blob {
    /// A text blob: `text` plus a trailing NUL.
    pub(crate) fn text(text: String) -> Self {
        let mut bytes = text.into_bytes();
        bytes.push(0);
        shadewire_blob { bytes, is_text: true }
    }

    /// A binary blob: the little-endian bytes of `words`.
    pub(crate) fn words(words: &[u32]) -> Self {
        let bytes = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        shadewire_blob { bytes, is_text: false }
    }

    fn len(&self) -> usize {
        self.bytes.len() - usize::from(self.is_text)
    }
}

/// The blob's bytes, valid until `b` is freed; writes their count to `len` when non-null (a text blob's trailing NUL
/// is not counted). NULL, with `len` 0, when `b` is NULL.
///
/// # Safety
/// `b` must be NULL or a live handle; `len` NULL or valid for writing a `size_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_blob_data(b: *const shadewire_blob, len: *mut usize) -> *const u8 {
    let (ptr, count) = match unsafe { b.as_ref() } {
        Some(blob) => (blob.bytes.as_ptr(), blob.len()),
        None => (std::ptr::null(), 0),
    };
    if !len.is_null() {
        unsafe { *len = count };
    }
    ptr
}

/// Whether the blob is text (NUL-terminated, UTF-8) rather than binary; false when `b` is NULL.
///
/// # Safety
/// `b` must be NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_blob_is_text(b: *const shadewire_blob) -> bool {
    unsafe { b.as_ref() }.is_some_and(|blob| blob.is_text)
}

/// Frees `b`; NULL is allowed.
///
/// # Safety
/// `b` must be NULL or a handle from this library not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_blob_free(b: *mut shadewire_blob) {
    if !b.is_null() {
        drop(unsafe { Box::from_raw(b) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_blobs_hide_the_nul() {
        let blob = shadewire_blob::text("abc".to_string());
        let mut len = 0usize;
        let ptr = unsafe { shadewire_blob_data(&blob, &mut len) };
        assert_eq!(len, 3);
        assert_eq!(unsafe { *ptr.add(3) }, 0);
        assert!(unsafe { shadewire_blob_is_text(&blob) });
    }

    #[test]
    fn words_are_little_endian() {
        let blob = shadewire_blob::words(&[0x0723_0203]);
        let mut len = 0usize;
        let ptr = unsafe { shadewire_blob_data(&blob, &mut len) };
        assert_eq!(unsafe { std::slice::from_raw_parts(ptr, len) }, [3, 2, 0x23, 7]);
        assert!(!unsafe { shadewire_blob_is_text(&blob) });
    }

    #[test]
    fn null_blob() {
        let mut len = 9usize;
        assert!(unsafe { shadewire_blob_data(std::ptr::null(), &mut len) }.is_null());
        assert_eq!(len, 0);
        assert!(!unsafe { shadewire_blob_is_text(std::ptr::null()) });
    }
}
