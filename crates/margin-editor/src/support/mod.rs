//! Shared runtime pieces: the page reader, the background worker, and the
//! restart into a newer build.
pub mod page;
#[cfg(not(target_arch = "wasm32"))]
pub mod restart;
#[cfg(target_arch = "wasm32")]
pub(crate) use crate::web::restart;
pub mod worker;
