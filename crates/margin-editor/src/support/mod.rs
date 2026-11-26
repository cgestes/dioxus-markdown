//! Shared runtime pieces: the page reader, the background worker, and the
//! restart into a newer build.
pub mod page;
pub(crate) mod restart;
pub mod worker;
