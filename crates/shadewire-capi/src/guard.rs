//! The panic barrier every fallible entry point runs its body through.

use std::panic::{AssertUnwindSafe, UnwindSafe, catch_unwind};

use crate::error::{CapiError, shadewire_error};
use crate::status::shadewire_status;

/// Runs `body`, turning `Err` and panics into `SHADEWIRE_ERROR` and writing the error object to `error` when non-null.
///
/// The error object is built under a second panic barrier; if that fails too, a fixed PANIC error is written.
///
/// # Safety
/// `error` must be NULL, or valid and aligned for writing a `*mut shadewire_error`.
pub(crate) unsafe fn guarded(
    error: *mut *mut shadewire_error,
    body: impl FnOnce() -> Result<(), CapiError> + UnwindSafe,
) -> shadewire_status {
    let outcome = catch_unwind(body);
    if let Ok(Ok(())) = outcome {
        return shadewire_status::SHADEWIRE_OK;
    }
    if !error.is_null() {
        let failure = catch_unwind(AssertUnwindSafe(|| match outcome {
            Ok(Ok(())) => unreachable!("success returned above"),
            Ok(Err(e)) => shadewire_error::from(e),
            Err(payload) => crate::error::panic(payload),
        }))
        .unwrap_or_else(|_| crate::error::reporting_panic());
        unsafe { *error = Box::into_raw(Box::new(failure)) };
    }
    shadewire_status::SHADEWIRE_ERROR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_is_ok() {
        assert_eq!(unsafe { guarded(std::ptr::null_mut(), || Ok(())) }, shadewire_status::SHADEWIRE_OK);
    }

    #[test]
    fn err_is_error() {
        let status = unsafe { guarded(std::ptr::null_mut(), || Err(CapiError::Argument("bad".into()))) };
        assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
    }

    #[test]
    fn a_panic_does_not_unwind_out() {
        let status = unsafe { guarded(std::ptr::null_mut(), || panic!("boom")) };
        assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
    }
}
