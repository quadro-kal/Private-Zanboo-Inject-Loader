#![no_std]
extern crate zil_core;
pub mod sandbox_escape;
pub use sandbox_escape::{SandboxEscaper, EscapeResult};
