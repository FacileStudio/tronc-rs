//! `tronc`, the Rust chassis for Facile Suite APIs.
//!
//! It gives a Rust app the HTTP behaviour of Go `tronc`: the same error envelope, the same health
//! answers, the same security headers, the same client-IP rule and the same SPA fallback. Each
//! part sits behind a feature: `http`, `health`, `spa`, `migrate`, `testdb`, `journal`, `apiref`.

#![deny(unused, dead_code)]
#![warn(missing_docs)]

pub mod env;
pub mod errors;
pub mod logger;
pub mod proxies;

#[cfg(feature = "apiref")]
pub mod apiref;
#[cfg(feature = "health")]
pub mod health;
#[cfg(feature = "health")]
pub mod healthcheck;
#[cfg(feature = "http")]
pub mod http;
#[cfg(feature = "journal")]
pub mod journal;
#[cfg(feature = "http")]
pub mod json;
#[cfg(feature = "migrate")]
pub mod migrate;
#[cfg(feature = "spa")]
pub mod spa;
#[cfg(feature = "testdb")]
pub mod testdb;

pub use errors::{Code, Error, Result};
