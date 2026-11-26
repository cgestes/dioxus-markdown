//! The personal notebook: on the encrypted vault with the `vault` feature,
//! otherwise the Field notes sample in memory.
#[cfg(feature = "vault")]
mod desktop;
#[cfg(feature = "vault")]
pub(crate) use desktop::*;

#[cfg(not(feature = "vault"))]
mod off;
#[cfg(not(feature = "vault"))]
pub(crate) use off::*;
