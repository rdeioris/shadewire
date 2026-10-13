//! Every handle may be read from many threads at once: a compile-time check that each is `Send + Sync`.

use crate::compiled::shadewire_compiled;
use crate::error::shadewire_error;
use crate::ir::shadewire_ir;
use crate::names::shadewire_strings;

/// Compiles only when `T` may move to and be shared between threads.
const fn send_sync<T: Send + Sync>() {}

const _: () = {
    send_sync::<shadewire_ir>();
    send_sync::<shadewire_compiled>();
    send_sync::<shadewire_error>();
    send_sync::<shadewire_strings>();
};
