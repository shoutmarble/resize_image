//! End-to-end tests for the native ffmpeg backend.
//! Every test skips (passing) when ffmpeg/ffprobe are not on PATH.

use std::path::{Path, PathBuf};
use std::process::Command;
use wasmffmpeg_gui::backend::native;

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Generates a 2560×1440 PNG still (larger than the default box).
fn sample_image(dir: &Path) -> PathBuf {
    let path = dir.join("sample.png");
    let status = Command::new("ffmpeg")
        .args(["-f", "lavfi", "-i", "testsrc=size=2560x1440", "-frames:v", "1", "-y"])
        .arg(&path)
        .status()
        .expect("run ffmpeg");
    assert!(status.success());
    path
}

/// Generates a 2 s 640×360 video WITH audio.
fn sample_video(dir: &Path) -> PathBuf {
    let path = dir.join("sample.mp4");
    let status = Command::new("ffmpeg")
        .args([
            "-f", "lavfi", "-i", "testsrc=size=640x360:duration=2:rate=15",
            "-f", "lavfi", "-i", "sine=frequency=440:duration=2",
            "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", "-y",
        ])
        .arg(&path)
        .status()
        .expect("run ffmpeg");
    assert!(status.success());
    path
}

#[test]
fn probe_reports_dimensions_and_duration() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg not found; skipping");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let video = sample_video(dir.path());
    let info = native::probe(&video).expect("probe video");
    assert_eq!(info.width, 640);
    assert_eq!(info.height, 360);
    let duration = info.duration_secs.expect("video has duration");
    assert!((duration - 2.0).abs() < 0.3, "duration {duration}");

    let image = sample_image(dir.path());
    let info = native::probe(&image).expect("probe image");
    assert_eq!((info.width, info.height), (2560, 1440));
}

#[test]
fn probe_rejects_garbage_file() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg not found; skipping");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let bogus = dir.path().join("bogus.mp4");
    std::fs::write(&bogus, b"not a video").unwrap();
    assert!(native::probe(&bogus).is_err());
}
