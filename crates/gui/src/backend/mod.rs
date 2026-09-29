//! Execution backends. v1 ships the native subprocess backend; a
//! ffmpeg.wasm backend for `wasm32` plugs into the same types later.

#[cfg(not(target_arch = "wasm32"))]
pub mod native;

/// Probed metadata for one input file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProbeInfo {
    pub width: u32,
    pub height: u32,
    /// Total duration in seconds; `None` for still images.
    pub duration_secs: Option<f64>,
}

/// Errors surfaced to the UI.
#[derive(Debug, Clone, PartialEq)]
pub enum BackendError {
    /// ffmpeg/ffprobe could not be started at all.
    Spawn(String),
    /// Probing failed (unreadable or corrupt input).
    ProbeFailed(String),
    /// ffmpeg exited unsuccessfully; carries the tail of stderr.
    Failed(String),
    /// The job was cancelled by the user.
    Cancelled,
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(e) => write!(f, "could not start ffmpeg: {e}"),
            Self::ProbeFailed(e) => write!(f, "could not read media file: {e}"),
            Self::Failed(e) => write!(f, "conversion failed: {e}"),
            Self::Cancelled => f.write_str("cancelled"),
        }
    }
}

impl std::error::Error for BackendError {}

/// Checks that both `ffmpeg` and `ffprobe` are available on PATH.
#[cfg(not(target_arch = "wasm32"))]
pub fn check_ffmpeg_available() -> Result<(), String> {
    native::check_available()
}
