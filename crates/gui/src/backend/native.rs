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
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height:format=duration",
            "-of",
            "json",
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
    Ok(ProbeInfo {
        width,
        height,
        duration_secs,
    })
}

/// Keeps only the last ~4 KiB of stderr for display.
pub(crate) fn tail(stderr: &str) -> String {
    const MAX: usize = 4096;
    let start = stderr.floor_char_boundary(stderr.len().saturating_sub(MAX));
    stderr[start..].trim().to_string()
}

use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use wasmffmpeg_core::{
    ConversionJob, OutputFormat, ProgressEvent, build_ffmpeg_args, candidate_name, fraction,
    parse_progress_line,
};

/// Picks a non-existing output path inside `dir`, incrementing a `_N`
/// suffix until the name is free. Never overwrites.
pub fn unique_output_path(dir: &Path, input: &Path, format: OutputFormat) -> PathBuf {
    let mut suffix = None;
    loop {
        let candidate = dir.join(candidate_name(input, format, suffix));
        if !candidate.exists() {
            return candidate;
        }
        suffix = Some(suffix.map_or(1, |n: u32| n + 1));
    }
}

/// Runs one conversion job. Reports progress as 0.0..=1.0 fractions (video
/// only; image jobs simply return at the end). The `Output` is the produced
/// file path on success.
///
/// Setting `cancel` kills ffmpeg and removes the partial output file.
pub fn convert(
    job: ConversionJob,
    total_duration_secs: Option<f64>,
    cancel: Arc<AtomicBool>,
) -> impl sipper::Sipper<Result<PathBuf, BackendError>, f32> {
    sipper::sipper(move |mut sender| async move {
        use futures::StreamExt;
        let (mut progress_tx, mut progress_rx) = futures::channel::mpsc::channel::<f32>(64);
        let (result_tx, result_rx) =
            futures::channel::oneshot::channel::<Result<PathBuf, BackendError>>();

        thread::spawn(move || {
            let result = run_ffmpeg(&job, total_duration_secs, &cancel, &mut progress_tx);
            let _ = result_tx.send(result);
        });

        while let Some(progress) = progress_rx.next().await {
            sender.send(progress).await;
        }
        result_rx
            .await
            .unwrap_or_else(|_| Err(BackendError::Failed("worker thread died".into())))
    })
}

fn run_ffmpeg(
    job: &ConversionJob,
    total_duration_secs: Option<f64>,
    cancel: &AtomicBool,
    progress_tx: &mut futures::channel::mpsc::Sender<f32>,
) -> Result<PathBuf, BackendError> {
    let mut child = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-nostdin",
            "-y",
            "-progress",
            "pipe:1",
            "-stats_period",
            "0.5",
        ])
        .args(build_ffmpeg_args(job))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| BackendError::Spawn(format!("failed to start ffmpeg: {e}")))?;

    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");
    // Drain stderr on its own thread so a full pipe buffer cannot deadlock ffmpeg.
    let stderr_reader = thread::spawn(move || {
        let mut buf = String::new();
        let _ = BufReader::new(stderr).read_to_string(&mut buf);
        buf
    });

    let mut cancelled = false;
    for line in BufReader::new(stdout).lines() {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            cancelled = true;
            break;
        }
        let Ok(line) = line else { break };
        match parse_progress_line(&line) {
            Some(ProgressEvent::OutTimeUs(us)) => {
                if let Some(fraction) = fraction(us, total_duration_secs) {
                    let _ = progress_tx.try_send(fraction);
                }
            }
            Some(ProgressEvent::End) => {
                let _ = progress_tx.try_send(1.0);
            }
            None => {}
        }
    }

    let status = child
        .wait()
        .map_err(|e| BackendError::Failed(format!("failed to wait on ffmpeg: {e}")))?;
    let stderr_text = stderr_reader.join().unwrap_or_default();

    if cancelled {
        let _ = std::fs::remove_file(&job.output);
        return Err(BackendError::Cancelled);
    }
    if !status.success() {
        let _ = std::fs::remove_file(&job.output);
        return Err(BackendError::Failed(tail(&stderr_text)));
    }
    Ok(job.output.clone())
}
