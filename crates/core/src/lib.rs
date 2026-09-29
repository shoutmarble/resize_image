//! wasmffmpeg-core: pure conversion logic shared by all execution backends.
//!
//! This crate performs no I/O and must stay compilable to
//! `wasm32-unknown-unknown`.

pub mod format;
pub mod job;
pub mod media;
pub mod resize;

pub use format::{ImageFormat, OutputFormat, VideoFormat};
pub use job::{ConversionJob, build_ffmpeg_args};
pub use media::MediaKind;
pub use resize::{ResizeSpec, effective_box, scale_filter};
