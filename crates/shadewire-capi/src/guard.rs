//! The panic barrier every fallible entry point runs its body through.

use std::panic::{UnwindSafe, catch_unwind};

use crate::error::{CapiError, shadewire_error};
use crate::status::shadewire_status;

/// Runs `body`, turning `Err` and panics into `SHADEWIRE_ERROR`. The error object is not written yet.
pub fn guarded(
    error: *mut *mut shadewire_error,
    body: impl FnOnce() -> Result<(), CapiError> + UnwindSafe,
) -> shadewire_status {
    let _ = error;
    match catch_unwind(body) {
        Ok(Ok(())) => shadewire_status::SHADEWIRE_OK,
        Ok(Err(_)) | Err(_) => shadewire_status::SHADEWIRE_ERROR,
    }
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
