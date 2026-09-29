//! wasmffmpeg-core: pure conversion logic shared by all execution backends.
//!
//! This crate performs no I/O and must stay compilable to
//! `wasm32-unknown-unknown`.

pub mod format;
pub mod media;

pub use format::{ImageFormat, OutputFormat, VideoFormat};
pub use media::MediaKind;
