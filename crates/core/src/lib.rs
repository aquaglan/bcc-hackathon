//! Domain, ports and use cases. No I/O here: this crate must not depend on
//! an HTTP client, a web framework or a runtime.

pub mod domain;
pub mod memory;
pub mod ports;
pub mod progress;
pub mod service;
