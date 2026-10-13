//! Includes resolved by the host: a C callback behind `shadewire_mtlx::Resolver`.

use std::ffi::c_void;
use std::os::raw::c_char;

use shadewire_mtlx::{Resolved, Resolver};

use crate::strings::{c_string, opt_cstr, text_arg};

/// Resolves one `xi:include`: `base` is the including document's location (as given to `shadewire_mtlx_to_ir`, or
/// as an earlier answer named it) and `href` the include's `href`, both NUL-terminated UTF-8 valid only during the
/// call. The callback answers through `out` with `shadewire_resolution_set` or `shadewire_resolution_fail` before it
/// returns; Shadewire copies what they are given, so the host keeps ownership of its buffers.
///
/// It is called only during the Shadewire call that received the options, on that call's thread, never after it
/// returns; `user_data` is passed through untouched. It must not unwind (throw or `longjmp`) into Shadewire.
pub type shadewire_resolve_fn = Option<
    unsafe extern "C" fn(
        user_data: *mut c_void,
        base: *const c_char,
        href: *const c_char,
        out: *mut shadewire_resolution,
    ),
>;

/// The answer a `shadewire_resolve_fn` gives, written with `shadewire_resolution_set` or `_fail`. Owned by Shadewire
/// and valid only during the callback.
pub struct shadewire_resolution {
    answer: Option<Result<Resolved, String>>,
}

/// The included document: `location` (NUL-terminated UTF-8; the base its own includes resolve against and the name
/// in messages) and `len` bytes of UTF-8 `text`, both copied before this returns. Replaces an earlier answer; a NULL
/// or non-UTF-8 argument becomes a failure. No-op when `out` is NULL.
///
/// # Safety
/// `out` must be NULL or the `shadewire_resolution` the callback was given; `location` NULL or a NUL-terminated
/// string; `text` NULL or `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_resolution_set(
    out: *mut shadewire_resolution,
    location: *const c_char,
    text: *const c_char,
    len: usize,
) {
    let Some(out) = (unsafe { out.as_mut() }) else { return };
    let location = unsafe { opt_cstr(location) }.map_err(|e| format!("resolved location: {}", e.0));
    let text = unsafe { text_arg(text, len) }.map_err(|e| format!("resolved text: {}", e.0));
    out.answer = Some(match (location, text) {
        (Ok(Some(location)), Ok(text)) => Ok(Resolved { location: location.to_string(), text: text.to_string() }),
        (Ok(None), _) => Err("resolved location: null pointer".to_string()),
        (Err(message), _) | (_, Err(message)) => Err(message),
    });
}

/// The include cannot be resolved: `message` (NUL-terminated UTF-8, copied; NULL for a generic one) becomes the
/// error the Shadewire call fails with. Replaces an earlier answer. No-op when `out` is NULL.
///
/// # Safety
/// `out` must be NULL or the `shadewire_resolution` the callback was given; `message` NULL or a NUL-terminated
/// string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_resolution_fail(out: *mut shadewire_resolution, message: *const c_char) {
    let Some(out) = (unsafe { out.as_mut() }) else { return };
    let message = match unsafe { opt_cstr(message) } {
        Ok(Some(message)) => message.to_string(),
        Ok(None) => "the resolver failed".to_string(),
        Err(e) => format!("resolver message: {}", e.0),
    };
    out.answer = Some(Err(message));
}

/// The callback and its `user_data`, as `Resolver` holds them.
struct Callback {
    resolve: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char, *mut shadewire_resolution),
    user_data: *mut c_void,
}

// SAFETY: a `Resolver` built from options lives inside one C API call, and the MaterialX front end calls it only on
// that call's thread; the header tells hosts so.
unsafe impl Send for Callback {}
unsafe impl Sync for Callback {}

impl Callback {
    fn resolve(&self, base: &str, href: &str) -> Result<Resolved, String> {
        let base = c_string(base.to_string());
        let href_c = c_string(href.to_string());
        let mut out = shadewire_resolution { answer: None };
        unsafe { (self.resolve)(self.user_data, base.as_ptr(), href_c.as_ptr(), &mut out) };
        out.answer.unwrap_or_else(|| Err(format!("the resolver gave no answer for {href}")))
    }
}

/// The resolver the options name: the host's callback, or the file system when it is NULL.
pub(crate) fn resolver(resolve: shadewire_resolve_fn, user_data: *mut c_void) -> Resolver {
    match resolve {
        Some(resolve) => {
            let callback = Callback { resolve, user_data };
            Resolver::new(move |base, href| callback.resolve(base, href))
        }
        None => Resolver::default(),
    }
}
