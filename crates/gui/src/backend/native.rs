//! Native backend: runs the system `ffmpeg`/`ffprobe` binaries.

use super::{BackendError, ProbeInfo};
use serde::Deserialize;
use std::path::Path;
use std::process::{Command, Stdio};

/// Verifies both binaries run.
pub fn check_available() -> Result<(), String> {
    for bin in ["ffmpeg", "ffprobe"] {
        let ok = Command::new(bin)
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            return Err(format!(
                "`{bin}` was not found on PATH. Install FFmpeg (e.g. `sudo apt install ffmpeg`) and restart the app."
            ));
        }
    }
    Ok(())
}

#[derive(Deserialize)]
struct ProbeOutput {
    streams: Option<Vec<ProbeStream>>,
    format: Option<ProbeFormat>,
}

#[derive(Deserialize)]
struct ProbeStream {
    width: Option<u32>,
    height: Option<u32>,
}

#[derive(Deserialize)]
struct ProbeFormat {
    duration: Option<String>,
}

/// Probes one input file with ffprobe. Blocking but fast (~50 ms); call it
/// from inside a `Task::perform` future, not from the UI update directly.
pub fn probe(input: &Path) -> Result<ProbeInfo, BackendError> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "error",
            "-select_streams", "v:0",
            "-show_entries", "stream=width,height:format=duration",
            "-of", "json",
        ])
        .arg(input)
        .output()
        .map_err(|e| BackendError::Spawn(format!("failed to start ffprobe: {e}")))?;
    if !output.status.success() {
        return Err(BackendError::ProbeFailed(tail(&String::from_utf8_lossy(
            &output.stderr,
        ))));
    }
    let parsed: ProbeOutput = serde_json::from_slice(&output.stdout)
        .map_err(|e| BackendError::ProbeFailed(format!("invalid ffprobe output: {e}")))?;
    let (width, height) = parsed
        .streams
        .as_ref()
        .and_then(|streams| streams.first())
        .and_then(|s| s.width.zip(s.height))
        .ok_or_else(|| BackendError::ProbeFailed("no video stream dimensions found".into()))?;
    let duration_secs = parsed
        .format
        .and_then(|f| f.duration)
        .and_then(|d| d.parse::<f64>().ok());
    Ok(ProbeInfo { width, height, duration_secs })
}

/// Keeps only the last ~4 KiB of stderr for display.
pub(crate) fn tail(stderr: &str) -> String {
    const MAX: usize = 4096;
    let start = stderr.floor_char_boundary(stderr.len().saturating_sub(MAX));
    stderr[start..].trim().to_string()
}
