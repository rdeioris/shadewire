//! The panic barrier every fallible entry point runs its body through.

use std::panic::{UnwindSafe, catch_unwind};

use crate::error::{CapiError, shadewire_error};
use crate::status::shadewire_status;

/// Runs `body`, turning `Err` and panics into `SHADEWIRE_ERROR` and writing the error object to `error` when non-null.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // `error` is checked for NULL; callers pass the C out-parameter
pub fn guarded(
    error: *mut *mut shadewire_error,
    body: impl FnOnce() -> Result<(), CapiError> + UnwindSafe,
) -> shadewire_status {
    let failure = match catch_unwind(body) {
        Ok(Ok(())) => return shadewire_status::SHADEWIRE_OK,
        Ok(Err(e)) => shadewire_error::from(e),
        Err(payload) => crate::error::panic(payload),
    };
    if !error.is_null() {
        unsafe { *error = Box::into_raw(Box::new(failure)) };
    }
    shadewire_status::SHADEWIRE_ERROR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_is_ok() {
        assert_eq!(guarded(std::ptr::null_mut(), || Ok(())), shadewire_status::SHADEWIRE_OK);
    }

    #[test]
    fn err_is_error() {
        let status = guarded(std::ptr::null_mut(), || Err(CapiError::Argument("bad".into())));
        assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
    }

    #[test]
    fn a_panic_does_not_unwind_out() {
        let status = guarded(std::ptr::null_mut(), || panic!("boom"));
        assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
    }
}
