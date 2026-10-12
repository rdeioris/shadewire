//! The status every fallible function returns.

/// Result of a fallible call: `SHADEWIRE_OK` or `SHADEWIRE_ERROR` (details in the error out-parameter).
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum shadewire_status {
    /// The call succeeded.
    SHADEWIRE_OK = 0,
    /// The call failed; see the error out-parameter.
    SHADEWIRE_ERROR = 1,
}
