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

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use wasmffmpeg_core::{
    ConversionJob, ImageFormat, OutputFormat, ResizeSpec, VideoFormat,
};

fn job(input: PathBuf, output: PathBuf, format: OutputFormat, w: u32, h: u32, upscale: bool) -> ConversionJob {
    ConversionJob {
        input,
        output,
        format,
        input_width: w,
        input_height: h,
        resize: ResizeSpec::new(1920, 1080, upscale),
    }
}

fn run_convert(j: ConversionJob, duration: Option<f64>, cancel: Arc<AtomicBool>) -> Result<PathBuf, String> {
    futures::executor::block_on(native::convert(j, duration, cancel)).map_err(|e| e.to_string())
}

#[test]
fn converts_image_to_all_image_formats() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg not found; skipping");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let input = sample_image(dir.path()); // 2560x1440
    for format in [ImageFormat::Jpeg, ImageFormat::Png, ImageFormat::WebP, ImageFormat::Avif] {
        let output = dir.path().join(format!("out.{}", format.extension()));
        let j = job(input.clone(), output.clone(), OutputFormat::Image(format), 2560, 1440, false);
        let produced = run_convert(j, None, Arc::new(AtomicBool::new(false)))
            .unwrap_or_else(|e| panic!("{format:?} failed: {e}"));
        assert!(produced.exists() && std::fs::metadata(&produced).unwrap().len() > 0);
        let info = native::probe(&produced).unwrap();
        // 2560x1440 is 16:9, so the fit is exact.
        assert_eq!((info.width, info.height), (1920, 1080), "{format:?}");
    }
}

#[test]
fn converts_video_to_all_video_formats() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg not found; skipping");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let input = sample_video(dir.path()); // 640x360, no upscale by default
    for format in [VideoFormat::Mp4H264, VideoFormat::Mp4Av1, VideoFormat::WebMVp9, VideoFormat::WebMAv1] {
        let output = dir.path().join(format!("out-{format:?}.{}", format.extension()));
        let j = job(input.clone(), output.clone(), OutputFormat::Video(format), 640, 360, false);
        let produced = run_convert(j, Some(2.0), Arc::new(AtomicBool::new(false)))
            .unwrap_or_else(|e| panic!("{format:?} failed: {e}"));
        assert!(produced.exists() && std::fs::metadata(&produced).unwrap().len() > 0);
        let info = native::probe(&produced).unwrap();
        // No upscale: output stays at native size.
        assert_eq!((info.width, info.height), (640, 360), "{format:?}");
    }
}

#[test]
fn upscale_flag_scales_small_video_to_1080p() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg not found; skipping");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let input = sample_video(dir.path());
    let output = dir.path().join("upscaled.mp4");
    let j = job(input, output.clone(), OutputFormat::Video(VideoFormat::Mp4H264), 640, 360, true);
    run_convert(j, Some(2.0), Arc::new(AtomicBool::new(false))).unwrap();
    let info = native::probe(&output).unwrap();
    assert_eq!((info.width, info.height), (1920, 1080));
}

#[test]
fn cancel_stops_conversion_and_removes_partial_output() {
    if !ffmpeg_available() {
        eprintln!("ffmpeg not found; skipping");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    // 60 s of video so the encode cannot finish before the cancel lands.
    let long_input = dir.path().join("long.mp4");
    let status = Command::new("ffmpeg")
        .args(["-f", "lavfi", "-i", "testsrc=size=640x360:duration=60:rate=10", "-c:v", "libx264", "-y"])
        .arg(&long_input)
        .status()
        .unwrap();
    assert!(status.success());

    let output = dir.path().join("cancelled.mp4");
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(400));
        flag.store(true, Ordering::Relaxed);
    });
    let j = job(long_input, output.clone(), OutputFormat::Video(VideoFormat::Mp4H264), 640, 360, false);
    let result = run_convert(j, Some(60.0), cancel);
    assert_eq!(result, Err("cancelled".to_string()));
    assert!(!output.exists(), "partial output must be removed");
}

#[test]
fn unique_output_path_never_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    let input = PathBuf::from("/in/photo.png");
    let format = OutputFormat::Image(ImageFormat::Jpeg);
    let first = native::unique_output_path(dir.path(), &input, format);
    assert_eq!(first, dir.path().join("photo.jpg"));
    std::fs::write(&first, b"x").unwrap();
    let second = native::unique_output_path(dir.path(), &input, format);
    assert_eq!(second, dir.path().join("photo_1.jpg"));
    std::fs::write(&second, b"x").unwrap();
    let third = native::unique_output_path(dir.path(), &input, format);
    assert_eq!(third, dir.path().join("photo_2.jpg"));
}
