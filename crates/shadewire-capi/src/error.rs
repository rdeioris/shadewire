//! Error objects (the opaque handle is filled in by a later task).

use crate::strings::ArgError;

/// An error returned by a failed call.
pub struct shadewire_error {
    _private: (),
}

/// Everything a C API body can fail with.
#[derive(Debug)]
pub enum CapiError {
    /// An error from the core library.
    Core(shadewire::Error),
    /// An invalid argument passed by the caller.
    Argument(String),
}

impl From<shadewire::Error> for CapiError {
    fn from(error: shadewire::Error) -> Self {
        CapiError::Core(error)
    }
}

impl From<ArgError> for CapiError {
    fn from(error: ArgError) -> Self {
        CapiError::Argument(error.0)
    }
}
