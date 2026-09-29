# Media Converter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `wasmffmpeg`, a native iced 0.14 desktop app that converts/resizes images (JPEG/PNG/WebP/AVIF) and videos (MP4 H.264/AV1, WebM VP9/AV1) to fit inside 1920×1080 preserving aspect ratio, single or bulk, with a WASM-ready pure-Rust core.

**Architecture:** Cargo workspace. `wasmffmpeg-core` is pure Rust with zero dependencies and no I/O (media types, format catalog, resize math, ffmpeg argument builder, progress parsing, name generation) and must compile to `wasm32-unknown-unknown`. `wasmffmpeg-gui` (lib + thin `main.rs`) holds the iced 0.14 app and the native backend, which runs system `ffmpeg`/`ffprobe` subprocesses inside a `sipper` so conversion progress streams into the UI via `Task::sip`.

**Tech Stack:** Rust 1.98 (edition 2024), iced 0.14.0 (`sipper` feature), sipper 0.1, rfd 0.17 (xdg-portal), futures 0.3, serde/serde_json, system ffmpeg 6.1.1.

**Spec:** `docs/superpowers/specs/2026-09-28-media-converter-design.md`

## Global Constraints

- `wasmffmpeg-core`: **no dependencies beyond `std`, no I/O** (no `std::fs`, no `std::process`, no network). `cargo check -p wasmffmpeg-core --target wasm32-unknown-unknown` must pass.
- iced is pinned to `0.14` with `features = ["sipper"]`; do not add other iced features.
- rfd is used as `rfd = { version = "0.17", default-features = false, features = ["xdg-portal"] }` (builds without system GTK dev libraries).
- ffmpeg recipes are copied **verbatim** from the spec's "Conversion recipes" section; do not improvise codec flags.
- Default resize: fit inside 1920×1080, `allow_upscale: false`. Video scale filter always ends with `:force_divisible_by=2`; image scale filter never does.
- Never overwrite existing output files: use `candidate_name` with incrementing suffixes.
- Sequential queue: exactly one ffmpeg process at a time; a failed or cancelled file never stops the queue.
- TDD: write the failing test first for every `core` module; integration tests for the backend must skip gracefully when ffmpeg is absent.
- Commit after every task with `git -c user.name=... -c user.email=...` (repo has no identity configured).

---

### Task 1: Workspace scaffold + core media/format types

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `.gitignore`
- Create: `crates/core/Cargo.toml`
- Create: `crates/core/src/lib.rs`
- Create: `crates/core/src/media.rs`
- Create: `crates/core/src/format.rs`

**Interfaces:**
- Produces:
  - `MediaKind { Image, Video }`, `MediaKind::from_extension(&str) -> Option<MediaKind>`, `MediaKind::from_path(&Path) -> Option<MediaKind>`, `IMAGE_EXTENSIONS: &[&str]`, `VIDEO_EXTENSIONS: &[&str]`
  - `ImageFormat { Jpeg, Png, WebP, Avif }` with `ImageFormat::ALL: &[ImageFormat]`
  - `VideoFormat { Mp4H264, Mp4Av1, WebMVp9, WebMAv1 }` with `VideoFormat::ALL: &[VideoFormat]`
  - `OutputFormat { Image(ImageFormat), Video(VideoFormat) }` with `all_for(MediaKind) -> &'static [OutputFormat]`, `kind(self) -> MediaKind`, `extension(self) -> &'static str`, `default_for(MediaKind) -> OutputFormat`; `Display` on all three (needed by `pick_list`)

- [ ] **Step 1: Verify the ffmpeg feature set** (recipe feasibility gate)

Run:
```bash
ffmpeg -hide_banner -encoders 2>/dev/null | grep -E 'libx264|libsvtav1|libvpx-vp9|libaom-av1|libwebp|libopus|aac' ; ffmpeg -hide_banner -muxers 2>/dev/null | grep -E '\b(avif|mp4|webm)\b'
```
Expected: one line per encoder (libx264, libsvtav1, libvpx-vp9, libaom-av1, libwebp, libopus, aac) and muxers avif, mp4, webm. If `avif` muxer is missing, STOP and report — the AVIF recipe needs ffmpeg ≥ 6.0 with the avif muxer.

- [ ] **Step 2: Write the workspace scaffold**

`Cargo.toml`:
```toml
[workspace]
resolver = "3"
members = ["crates/core", "crates/gui"]

[workspace.package]
edition = "2024"
license = "MIT OR Apache-2.0"

[profile.release]
lto = true
```

`.gitignore`:
```
/target
```

`crates/core/Cargo.toml`:
```toml
[package]
name = "wasmffmpeg-core"
edition.workspace = true
license.workspace = true

[dependencies]
```

`crates/gui` does not exist yet; temporarily comment it out of `members` until Task 6: `members = ["crates/core"]`.

- [ ] **Step 3: Write the failing tests** — append to a fresh `crates/core/src/media.rs` / `format.rs` (tests in-file, `#[cfg(test)] mod tests`):

`media.rs` tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_image_extensions_case_insensitively() {
        for ext in ["jpg", "jpeg", "png", "webp", "avif", "JPG", "Png", ".webp"] {
            assert_eq!(MediaKind::from_extension(ext), Some(MediaKind::Image), "{ext}");
        }
    }

    #[test]
    fn detects_video_extensions() {
        for ext in ["mp4", "mov", "mkv", "webm", "AVI"] {
            assert_eq!(MediaKind::from_extension(ext), Some(MediaKind::Video), "{ext}");
        }
    }

    #[test]
    fn rejects_unknown_extensions() {
        assert_eq!(MediaKind::from_extension("txt"), None);
        assert_eq!(MediaKind::from_extension(""), None);
        assert_eq!(MediaKind::from_path(std::path::Path::new("no_extension")), None);
    }

    #[test]
    fn detects_from_path() {
        assert_eq!(MediaKind::from_path(std::path::Path::new("/a/b/photo.JPEG")), Some(MediaKind::Image));
        assert_eq!(MediaKind::from_path(std::path::Path::new("movie.mkv")), Some(MediaKind::Video));
    }
}
```

`format.rs` tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::MediaKind;

    #[test]
    fn image_format_extensions() {
        assert_eq!(ImageFormat::Jpeg.extension(), "jpg");
        assert_eq!(ImageFormat::Png.extension(), "png");
        assert_eq!(ImageFormat::WebP.extension(), "webp");
        assert_eq!(ImageFormat::Avif.extension(), "avif");
    }

    #[test]
    fn video_format_extensions() {
        assert_eq!(VideoFormat::Mp4H264.extension(), "mp4");
        assert_eq!(VideoFormat::Mp4Av1.extension(), "mp4");
        assert_eq!(VideoFormat::WebMVp9.extension(), "webm");
        assert_eq!(VideoFormat::WebMAv1.extension(), "webm");
    }

    #[test]
    fn all_for_returns_kind_formats() {
        assert_eq!(OutputFormat::all_for(MediaKind::Image).len(), 4);
        assert_eq!(OutputFormat::all_for(MediaKind::Video).len(), 4);
        assert!(OutputFormat::all_for(MediaKind::Image).iter().all(|f| f.kind() == MediaKind::Image));
        assert!(OutputFormat::all_for(MediaKind::Video).iter().all(|f| f.kind() == MediaKind::Video));
    }

    #[test]
    fn defaults() {
        assert_eq!(OutputFormat::default_for(MediaKind::Image), OutputFormat::Image(ImageFormat::Jpeg));
        assert_eq!(OutputFormat::default_for(MediaKind::Video), OutputFormat::Video(VideoFormat::Mp4H264));
    }

    #[test]
    fn display_labels() {
        assert_eq!(ImageFormat::WebP.to_string(), "WebP");
        assert_eq!(VideoFormat::Mp4H264.to_string(), "MP4 / H.264");
        assert_eq!(VideoFormat::WebMAv1.to_string(), "WebM / AV1");
        assert_eq!(OutputFormat::Image(ImageFormat::Avif).to_string(), "AVIF");
    }
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test -p wasmffmpeg-core`
Expected: FAIL — `media`/`format` modules do not exist.

- [ ] **Step 5: Implement `media.rs`**

```rust
//! Media kind detection from file extensions.

/// The broad category of a media file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaKind {
    Image,
    Video,
}

/// Extensions accepted as image inputs (lowercase, without dot).
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "avif", "bmp", "gif", "tiff", "tif",
];

/// Extensions accepted as video inputs (lowercase, without dot).
pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "mkv", "webm", "avi", "mpg", "mpeg", "ts", "m2ts", "3gp", "flv", "wmv",
    "vob", "ogv",
];

impl MediaKind {
    /// Detects the media kind from an extension string (case-insensitive,
    /// leading dot allowed).
    pub fn from_extension(ext: &str) -> Option<MediaKind> {
        let ext = ext.trim_start_matches('.').to_ascii_lowercase();
        if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
            Some(MediaKind::Image)
        } else if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
            Some(MediaKind::Video)
        } else {
            None
        }
    }

    /// Detects the media kind from a path's extension.
    pub fn from_path(path: &std::path::Path) -> Option<MediaKind> {
        path.extension()
            .and_then(|e| e.to_str())
            .and_then(MediaKind::from_extension)
    }
}
```

- [ ] **Step 6: Implement `format.rs`**

```rust
//! Output format catalog.

use crate::media::MediaKind;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImageFormat {
    Jpeg,
    Png,
    WebP,
    Avif,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoFormat {
    Mp4H264,
    Mp4Av1,
    WebMVp9,
    WebMAv1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputFormat {
    Image(ImageFormat),
    Video(VideoFormat),
}

impl ImageFormat {
    pub const ALL: &[ImageFormat] = &[Self::Jpeg, Self::Png, Self::WebP, Self::Avif];

    pub fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::WebP => "webp",
            Self::Avif => "avif",
        }
    }
}

impl VideoFormat {
    pub const ALL: &[VideoFormat] =
        &[Self::Mp4H264, Self::Mp4Av1, Self::WebMVp9, Self::WebMAv1];

    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4H264 | Self::Mp4Av1 => "mp4",
            Self::WebMVp9 | Self::WebMAv1 => "webm",
        }
    }
}

const IMAGE_OUTPUT_FORMATS: &[OutputFormat] = &[
    OutputFormat::Image(ImageFormat::Jpeg),
    OutputFormat::Image(ImageFormat::Png),
    OutputFormat::Image(ImageFormat::WebP),
    OutputFormat::Image(ImageFormat::Avif),
];

const VIDEO_OUTPUT_FORMATS: &[OutputFormat] = &[
    OutputFormat::Video(VideoFormat::Mp4H264),
    OutputFormat::Video(VideoFormat::Mp4Av1),
    OutputFormat::Video(VideoFormat::WebMVp9),
    OutputFormat::Video(VideoFormat::WebMAv1),
];

impl OutputFormat {
    /// All selectable output formats for a media kind, in menu order.
    pub fn all_for(kind: MediaKind) -> &'static [OutputFormat] {
        match kind {
            MediaKind::Image => IMAGE_OUTPUT_FORMATS,
            MediaKind::Video => VIDEO_OUTPUT_FORMATS,
        }
    }

    /// The preselected output format for newly added files of a kind.
    pub fn default_for(kind: MediaKind) -> OutputFormat {
        match kind {
            MediaKind::Image => OutputFormat::Image(ImageFormat::Jpeg),
            MediaKind::Video => OutputFormat::Video(VideoFormat::Mp4H264),
        }
    }

    pub fn kind(self) -> MediaKind {
        match self {
            OutputFormat::Image(_) => MediaKind::Image,
            OutputFormat::Video(_) => MediaKind::Video,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            OutputFormat::Image(f) => f.extension(),
            OutputFormat::Video(f) => f.extension(),
        }
    }
}

impl fmt::Display for ImageFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Jpeg => "JPEG",
            Self::Png => "PNG",
            Self::WebP => "WebP",
            Self::Avif => "AVIF",
        })
    }
}

impl fmt::Display for VideoFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Mp4H264 => "MP4 / H.264",
            Self::Mp4Av1 => "MP4 / AV1",
            Self::WebMVp9 => "WebM / VP9",
            Self::WebMAv1 => "WebM / AV1",
        })
    }
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputFormat::Image(inner) => inner.fmt(f),
            OutputFormat::Video(inner) => inner.fmt(f),
        }
    }
}
```

- [ ] **Step 7: Implement `lib.rs` (partial — later tasks append)**

```rust
//! wasmffmpeg-core: pure conversion logic shared by all execution backends.
//!
//! This crate performs no I/O and must stay compilable to
//! `wasm32-unknown-unknown`.

pub mod format;
pub mod media;

pub use format::{ImageFormat, OutputFormat, VideoFormat};
pub use media::MediaKind;
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test -p wasmffmpeg-core`
Expected: PASS, 9 tests.

- [ ] **Step 9: Commit**

```bash
git add -A
git -c user.name=dev -c user.email=dev@local commit -m "feat(core): media kind detection and output format catalog"
```

---

### Task 2: Resize math (`resize.rs`)

**Files:**
- Create: `crates/core/src/resize.rs`
- Modify: `crates/core/src/lib.rs` (add module + re-exports)

**Interfaces:**
- Produces: `ResizeSpec { width: u32, height: u32, allow_upscale: bool }` (`Default` = 1920×1080, no upscale; `ResizeSpec::new(w, h, allow_upscale)`), `effective_box(input_w: u32, input_h: u32, spec: &ResizeSpec) -> (u32, u32)`, `scale_filter(box_w: u32, box_h: u32, even_only: bool) -> String`
- Consumed by: Task 3 (`job.rs`) and Task 8 (`Preset::spec`)

- [ ] **Step 1: Write the failing tests** in `crates/core/src/resize.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> ResizeSpec {
        ResizeSpec::default()
    }

    #[test]
    fn default_is_1080p_no_upscale() {
        assert_eq!(spec(), ResizeSpec { width: 1920, height: 1080, allow_upscale: false });
    }

    #[test]
    fn larger_input_keeps_full_box() {
        assert_eq!(effective_box(3840, 2160, &spec()), (1920, 1080));
    }

    #[test]
    fn smaller_input_clamps_box_to_input() {
        assert_eq!(effective_box(1000, 500, &spec()), (1000, 500));
    }

    #[test]
    fn clamping_is_per_dimension() {
        assert_eq!(effective_box(2000, 500, &spec()), (1920, 500));
        assert_eq!(effective_box(1000, 2000, &spec()), (1000, 1080));
    }

    #[test]
    fn allow_upscale_never_clamps() {
        let up = ResizeSpec::new(1920, 1080, true);
        assert_eq!(effective_box(1000, 500, &up), (1920, 1080));
    }

    #[test]
    fn video_filter_forces_divisible_by_two() {
        assert_eq!(
            scale_filter(1920, 1080, true),
            "scale=w=1920:h=1080:force_original_aspect_ratio=decrease:force_divisible_by=2"
        );
    }

    #[test]
    fn image_filter_omits_divisible_by_two() {
        assert_eq!(
            scale_filter(1000, 500, false),
            "scale=w=1000:h=500:force_original_aspect_ratio=decrease"
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p wasmffmpeg-core resize`
Expected: FAIL — module `resize` not found (add `pub mod resize;` to lib.rs first, leave file missing → compile error is the "fail").

- [ ] **Step 3: Implement `resize.rs`** (above the test module):

```rust
//! Aspect-ratio-preserving resize box computation.

/// Requested output bounding box. The output fits *inside* the box,
/// preserving the input's aspect ratio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResizeSpec {
    pub width: u32,
    pub height: u32,
    pub allow_upscale: bool,
}

impl ResizeSpec {
    pub fn new(width: u32, height: u32, allow_upscale: bool) -> Self {
        Self { width, height, allow_upscale }
    }
}

impl Default for ResizeSpec {
    fn default() -> Self {
        Self { width: 1920, height: 1080, allow_upscale: false }
    }
}

/// Effective bounding box handed to the ffmpeg `scale` filter.
///
/// With upscaling disabled, each box dimension is clamped to the input
/// dimension, so a smaller input is re-encoded at (at most) its native
/// size instead of being blown up.
pub fn effective_box(input_w: u32, input_h: u32, spec: &ResizeSpec) -> (u32, u32) {
    if spec.allow_upscale {
        (spec.width, spec.height)
    } else {
        (spec.width.min(input_w), spec.height.min(input_h))
    }
}

/// Builds the ffmpeg `scale` filter expression fitting inside the box while
/// preserving aspect ratio. `even_only` adds `force_divisible_by=2`, which
/// yuv420p video codecs require.
pub fn scale_filter(box_w: u32, box_h: u32, even_only: bool) -> String {
    let mut filter =
        format!("scale=w={box_w}:h={box_h}:force_original_aspect_ratio=decrease");
    if even_only {
        filter.push_str(":force_divisible_by=2");
    }
    filter
}
```

Add to `lib.rs`: `pub mod resize;` and `pub use resize::{ResizeSpec, effective_box, scale_filter};`

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p wasmffmpeg-core`
Expected: PASS (16 tests total).

- [ ] **Step 5: Commit**

```bash
git add -A && git -c user.name=dev -c user.email=dev@local commit -m "feat(core): resize box math and scale filter builder"
```

---

### Task 3: ffmpeg argument builder (`job.rs`)

**Files:**
- Create: `crates/core/src/job.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Consumes: `ImageFormat`, `OutputFormat`, `VideoFormat` (Task 1); `ResizeSpec`, `effective_box`, `scale_filter` (Task 2)
- Produces: `ConversionJob { input: PathBuf, output: PathBuf, format: OutputFormat, input_width: u32, input_height: u32, resize: ResizeSpec }`, `build_ffmpeg_args(&ConversionJob) -> Vec<String>` — used verbatim by the native backend (Task 7)

- [ ] **Step 1: Write the failing tests** in `crates/core/src/job.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{ImageFormat, OutputFormat, VideoFormat};
    use crate::resize::ResizeSpec;
    use std::path::PathBuf;

    fn job(format: OutputFormat, w: u32, h: u32, allow_upscale: bool) -> ConversionJob {
        ConversionJob {
            input: PathBuf::from("/in/src.mkv"),
            output: PathBuf::from("/out/dst.bin"),
            format,
            input_width: w,
            input_height: h,
            resize: ResizeSpec::new(1920, 1080, allow_upscale),
        }
    }

    fn strings(args: Vec<String>) -> Vec<String> {
        args
    }

    #[test]
    fn jpeg_args() {
        let args = job(OutputFormat::Image(ImageFormat::Jpeg), 3840, 2160, false);
        assert_eq!(
            strings(build_ffmpeg_args(&args)),
            vec![
                "-i", "/in/src.mkv",
                "-vf", "scale=w=1920:h=1080:force_original_aspect_ratio=decrease",
                "-frames:v", "1",
                "-q:v", "2",
                "/out/dst.bin",
            ]
        );
    }

    #[test]
    fn png_args_have_no_codec_flags() {
        let args = job(OutputFormat::Image(ImageFormat::Png), 3840, 2160, false);
        assert_eq!(
            strings(build_ffmpeg_args(&args)),
            vec![
                "-i", "/in/src.mkv",
                "-vf", "scale=w=1920:h=1080:force_original_aspect_ratio=decrease",
                "-frames:v", "1",
                "/out/dst.bin",
            ]
        );
    }

    #[test]
    fn webp_args() {
        let args = job(OutputFormat::Image(ImageFormat::WebP), 3840, 2160, false);
        assert_eq!(
            strings(build_ffmpeg_args(&args)),
            vec![
                "-i", "/in/src.mkv",
                "-vf", "scale=w=1920:h=1080:force_original_aspect_ratio=decrease",
                "-frames:v", "1",
                "-c:v", "libwebp", "-quality", "85",
                "/out/dst.bin",
            ]
        );
    }

    #[test]
    fn avif_args() {
        let args = job(OutputFormat::Image(ImageFormat::Avif), 3840, 2160, false);
        assert_eq!(
            strings(build_ffmpeg_args(&args)),
            vec![
                "-i", "/in/src.mkv",
                "-vf", "scale=w=1920:h=1080:force_original_aspect_ratio=decrease",
                "-frames:v", "1",
                "-c:v", "libaom-av1", "-still-picture", "1", "-crf", "30", "-b:v", "0",
                "/out/dst.bin",
            ]
        );
    }

    #[test]
    fn h264_args() {
        let args = job(OutputFormat::Video(VideoFormat::Mp4H264), 3840, 2160, false);
        assert_eq!(
            strings(build_ffmpeg_args(&args)),
            vec![
                "-i", "/in/src.mkv",
                "-vf", "scale=w=1920:h=1080:force_original_aspect_ratio=decrease:force_divisible_by=2",
                "-c:v", "libx264", "-preset", "medium", "-crf", "20",
                "-pix_fmt", "yuv420p",
                "-c:a", "aac", "-b:a", "160k",
                "-movflags", "+faststart",
                "/out/dst.bin",
            ]
        );
    }

    #[test]
    fn mp4_av1_args() {
        let args = job(OutputFormat::Video(VideoFormat::Mp4Av1), 3840, 2160, false);
        assert_eq!(
            strings(build_ffmpeg_args(&args)),
            vec![
                "-i", "/in/src.mkv",
                "-vf", "scale=w=1920:h=1080:force_original_aspect_ratio=decrease:force_divisible_by=2",
                "-c:v", "libsvtav1", "-preset", "6", "-crf", "30",
                "-pix_fmt", "yuv420p",
                "-c:a", "aac", "-b:a", "160k",
                "-movflags", "+faststart",
                "/out/dst.bin",
            ]
        );
    }

    #[test]
    fn webm_vp9_args() {
        let args = job(OutputFormat::Video(VideoFormat::WebMVp9), 3840, 2160, false);
        assert_eq!(
            strings(build_ffmpeg_args(&args)),
            vec![
                "-i", "/in/src.mkv",
                "-vf", "scale=w=1920:h=1080:force_original_aspect_ratio=decrease:force_divisible_by=2",
                "-c:v", "libvpx-vp9", "-crf", "32", "-b:v", "0", "-row-mt", "1",
                "-pix_fmt", "yuv420p",
                "-c:a", "libopus", "-b:a", "128k",
                "/out/dst.bin",
            ]
        );
    }

    #[test]
    fn webm_av1_args() {
        let args = job(OutputFormat::Video(VideoFormat::WebMAv1), 3840, 2160, false);
        assert_eq!(
            strings(build_ffmpeg_args(&args)),
            vec![
                "-i", "/in/src.mkv",
                "-vf", "scale=w=1920:h=1080:force_original_aspect_ratio=decrease:force_divisible_by=2",
                "-c:v", "libsvtav1", "-preset", "6", "-crf", "32",
                "-pix_fmt", "yuv420p",
                "-c:a", "libopus", "-b:a", "128k",
                "/out/dst.bin",
            ]
        );
    }

    #[test]
    fn no_upscale_clamps_box_into_filter() {
        let args = job(OutputFormat::Video(VideoFormat::Mp4H264), 1000, 500, false);
        assert!(build_ffmpeg_args(&args).contains(
            &"scale=w=1000:h=500:force_original_aspect_ratio=decrease:force_divisible_by=2".to_string()
        ));
    }

    #[test]
    fn allow_upscale_uses_full_box() {
        let args = job(OutputFormat::Video(VideoFormat::Mp4H264), 1000, 500, true);
        assert!(build_ffmpeg_args(&args).contains(
            &"scale=w=1920:h=1080:force_original_aspect_ratio=decrease:force_divisible_by=2".to_string()
        ));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p wasmffmpeg-core job`
Expected: FAIL — module `job` not found (add `pub mod job;` to lib.rs first).

- [ ] **Step 3: Implement `job.rs`** (above the test module):

```rust
//! Conversion job description and ffmpeg argument construction.

use crate::format::{ImageFormat, OutputFormat, VideoFormat};
use crate::resize::{self, ResizeSpec};
use std::path::PathBuf;

/// Everything a backend needs to run one conversion.
#[derive(Debug, Clone)]
pub struct ConversionJob {
    pub input: PathBuf,
    pub output: PathBuf,
    pub format: OutputFormat,
    pub input_width: u32,
    pub input_height: u32,
    pub resize: ResizeSpec,
}

/// Builds the ffmpeg argument list for a job: everything after the
/// executable name, starting with `-i <input>` and ending with the output
/// path. Global flags (`-y`, `-progress`, …) are prepended by the backend.
pub fn build_ffmpeg_args(job: &ConversionJob) -> Vec<String> {
    let (box_w, box_h) =
        resize::effective_box(job.input_width, job.input_height, &job.resize);
    let mut args = vec!["-i".into(), job.input.to_string_lossy().into_owned()];
    match job.format {
        OutputFormat::Image(format) => {
            args.push("-vf".into());
            args.push(resize::scale_filter(box_w, box_h, false));
            args.extend(["-frames:v".into(), "1".into()]);
            match format {
                ImageFormat::Jpeg => args.extend(["-q:v".into(), "2".into()]),
                ImageFormat::Png => {}
                ImageFormat::WebP => args.extend([
                    "-c:v".into(),
                    "libwebp".into(),
                    "-quality".into(),
                    "85".into(),
                ]),
                ImageFormat::Avif => args.extend([
                    "-c:v".into(),
                    "libaom-av1".into(),
                    "-still-picture".into(),
                    "1".into(),
                    "-crf".into(),
                    "30".into(),
                    "-b:v".into(),
                    "0".into(),
                ]),
            }
        }
        OutputFormat::Video(format) => {
            args.push("-vf".into());
            args.push(resize::scale_filter(box_w, box_h, true));
            match format {
                VideoFormat::Mp4H264 => args.extend([
                    "-c:v".into(),
                    "libx264".into(),
                    "-preset".into(),
                    "medium".into(),
                    "-crf".into(),
                    "20".into(),
                    "-pix_fmt".into(),
                    "yuv420p".into(),
                    "-c:a".into(),
                    "aac".into(),
                    "-b:a".into(),
                    "160k".into(),
                    "-movflags".into(),
                    "+faststart".into(),
                ]),
                VideoFormat::Mp4Av1 => args.extend([
                    "-c:v".into(),
                    "libsvtav1".into(),
                    "-preset".into(),
                    "6".into(),
                    "-crf".into(),
                    "30".into(),
                    "-pix_fmt".into(),
                    "yuv420p".into(),
                    "-c:a".into(),
                    "aac".into(),
                    "-b:a".into(),
                    "160k".into(),
                    "-movflags".into(),
                    "+faststart".into(),
                ]),
                VideoFormat::WebMVp9 => args.extend([
                    "-c:v".into(),
                    "libvpx-vp9".into(),
                    "-crf".into(),
                    "32".into(),
                    "-b:v".into(),
                    "0".into(),
                    "-row-mt".into(),
                    "1".into(),
                    "-pix_fmt".into(),
                    "yuv420p".into(),
                    "-c:a".into(),
                    "libopus".into(),
                    "-b:a".into(),
                    "128k".into(),
                ]),
                VideoFormat::WebMAv1 => args.extend([
                    "-c:v".into(),
                    "libsvtav1".into(),
                    "-preset".into(),
                    "6".into(),
                    "-crf".into(),
                    "32".into(),
                    "-pix_fmt".into(),
                    "yuv420p".into(),
                    "-c:a".into(),
                    "libopus".into(),
                    "-b:a".into(),
                    "128k".into(),
                ]),
            }
        }
    }
    args.push(job.output.to_string_lossy().into_owned());
    args
}
```

Add to `lib.rs`: `pub mod job;` and `pub use job::{ConversionJob, build_ffmpeg_args};`

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p wasmffmpeg-core`
Expected: PASS (26 tests total).

- [ ] **Step 5: Commit**

```bash
git add -A && git -c user.name=dev -c user.email=dev@local commit -m "feat(core): ffmpeg argument builder with per-format recipes"
```

---

### Task 4: Progress parsing (`progress.rs`)

**Files:**
- Create: `crates/core/src/progress.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Produces: `ProgressEvent { OutTimeUs(u64), End }`, `parse_progress_line(&str) -> Option<ProgressEvent>`, `fraction(out_time_us: u64, total_duration_secs: Option<f64>) -> Option<f32>`
- Consumed by: Task 7 (native backend reads `-progress pipe:1` output)

- [ ] **Step 1: Write the failing tests** in `crates/core/src/progress.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_out_time_us() {
        assert_eq!(parse_progress_line("out_time_us=1500000"), Some(ProgressEvent::OutTimeUs(1_500_000)));
    }

    #[test]
    fn out_time_ms_is_microseconds_too() {
        // ffmpeg's historical naming bug: out_time_ms is also microseconds.
        assert_eq!(parse_progress_line("out_time_ms=250000"), Some(ProgressEvent::OutTimeUs(250_000)));
    }

    #[test]
    fn parses_end_marker() {
        assert_eq!(parse_progress_line("progress=end"), Some(ProgressEvent::End));
        assert_eq!(parse_progress_line("progress=continue"), None);
    }

    #[test]
    fn ignores_unrelated_and_malformed_lines() {
        assert_eq!(parse_progress_line("frame=42"), None);
        assert_eq!(parse_progress_line("out_time_us=abc"), None);
        assert_eq!(parse_progress_line("no equals sign"), None);
        assert_eq!(parse_progress_line(""), None);
    }

    #[test]
    fn fraction_of_total() {
        assert_eq!(fraction(1_000_000, Some(4.0)), Some(0.25));
        assert_eq!(fraction(2_000_000, Some(2.0)), Some(1.0));
    }

    #[test]
    fn fraction_clamps_and_handles_unknown_total() {
        assert_eq!(fraction(5_000_000, Some(2.0)), Some(1.0));
        assert_eq!(fraction(1_000_000, None), None);
        assert_eq!(fraction(1_000_000, Some(0.0)), None);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p wasmffmpeg-core progress`
Expected: FAIL — module `progress` not found (add `pub mod progress;` to lib.rs first).

- [ ] **Step 3: Implement `progress.rs`** (above the test module):

```rust
//! Parsing of ffmpeg `-progress pipe:1` key=value output.

/// A parsed line of ffmpeg progress output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressEvent {
    /// Processed media time so far, in microseconds.
    OutTimeUs(u64),
    /// `progress=end` — all input was processed.
    End,
}

/// Parses one line of `-progress` output. Returns `None` for unrelated or
/// malformed lines (callers read line-by-line and simply skip them).
pub fn parse_progress_line(line: &str) -> Option<ProgressEvent> {
    let (key, value) = line.split_once('=')?;
    match key.trim() {
        // `out_time_ms` is a historical alias that is also in microseconds.
        "out_time_us" | "out_time_ms" => {
            value.trim().parse::<u64>().ok().map(ProgressEvent::OutTimeUs)
        }
        "progress" if value.trim() == "end" => Some(ProgressEvent::End),
        _ => None,
    }
}

/// Converts processed time into a 0.0..=1.0 fraction of the total duration.
/// Returns `None` when the total duration is unknown or non-positive.
pub fn fraction(out_time_us: u64, total_duration_secs: Option<f64>) -> Option<f32> {
    let total = total_duration_secs?;
    if total <= 0.0 {
        return None;
    }
    Some((out_time_us as f64 / 1_000_000.0 / total).clamp(0.0, 1.0) as f32)
}
```

Add to `lib.rs`: `pub mod progress;` and `pub use progress::{ProgressEvent, fraction, parse_progress_line};`

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p wasmffmpeg-core`
Expected: PASS (32 tests total).

- [ ] **Step 5: Commit**

```bash
git add -A && git -c user.name=dev -c user.email=dev@local commit -m "feat(core): ffmpeg progress output parsing"
```

---

### Task 5: Output name generation (`naming.rs`)

**Files:**
- Create: `crates/core/src/naming.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Produces: `candidate_name(input: &Path, format: OutputFormat, suffix: Option<u32>) -> String`
- Consumed by: Task 7 `unique_output_path` (which supplies the filesystem collision check)

- [ ] **Step 1: Write the failing tests** in `crates/core/src/naming.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{ImageFormat, OutputFormat, VideoFormat};
    use std::path::Path;

    #[test]
    fn replaces_extension_with_format_extension() {
        assert_eq!(
            candidate_name(Path::new("/a/b/photo.png"), OutputFormat::Image(ImageFormat::Jpeg), None),
            "photo.jpg"
        );
        assert_eq!(
            candidate_name(Path::new("movie.mkv"), OutputFormat::Video(VideoFormat::WebMAv1), None),
            "movie.webm"
        );
    }

    #[test]
    fn suffix_goes_before_extension() {
        assert_eq!(
            candidate_name(Path::new("photo.png"), OutputFormat::Image(ImageFormat::WebP), Some(1)),
            "photo_1.webp"
        );
        assert_eq!(
            candidate_name(Path::new("photo.png"), OutputFormat::Image(ImageFormat::WebP), Some(12)),
            "photo_12.webp"
        );
    }

    #[test]
    fn handles_multi_dot_and_extensionless_names() {
        assert_eq!(
            candidate_name(Path::new("archive.tar.gz"), OutputFormat::Image(ImageFormat::Png), None),
            "archive.tar.png"
        );
        assert_eq!(
            candidate_name(Path::new("noext"), OutputFormat::Video(VideoFormat::Mp4H264), None),
            "noext.mp4"
        );
    }

    #[test]
    fn falls_back_when_no_stem() {
        assert_eq!(
            candidate_name(Path::new("/"), OutputFormat::Image(ImageFormat::Jpeg), None),
            "output.jpg"
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p wasmffmpeg-core naming`
Expected: FAIL — module `naming` not found (add `pub mod naming;` to lib.rs first).

- [ ] **Step 3: Implement `naming.rs`** (above the test module):

```rust
//! Output file name generation.

use crate::format::OutputFormat;
use std::path::Path;

/// Generates the output file name (no directory component) for an input
/// file and target format. `suffix` appends `_N` before the extension for
/// collision avoidance; the caller performs the actual filesystem check and
/// increments the suffix. Inputs without a usable stem produce `"output"`.
pub fn candidate_name(input: &Path, format: OutputFormat, suffix: Option<u32>) -> String {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("output");
    let suffix = suffix.map(|n| format!("_{n}")).unwrap_or_default();
    format!("{stem}{suffix}.{}", format.extension())
}
```

Add to `lib.rs`: `pub mod naming;` and `pub use naming::candidate_name;`

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p wasmffmpeg-core`
Expected: PASS (36 tests total).

- [ ] **Step 5: Verify the wasm target compiles (WASM-readiness gate)**

Run:
```bash
rustup target add wasm32-unknown-unknown
cargo check -p wasmffmpeg-core --target wasm32-unknown-unknown
```
Expected: PASS with no errors. If `rustup` fails offline, note it and continue — but the code must stay `std`-only.

- [ ] **Step 6: Commit**

```bash
git add -A && git -c user.name=dev -c user.email=dev@local commit -m "feat(core): output name generation"
```

---

### Task 6: GUI crate skeleton + native backend probe

**Files:**
- Create: `crates/gui/Cargo.toml`
- Create: `crates/gui/src/lib.rs`
- Create: `crates/gui/src/main.rs`
- Create: `crates/gui/src/backend/mod.rs`
- Create: `crates/gui/src/backend/native.rs`
- Create: `crates/gui/tests/native_backend.rs`
- Modify: `Cargo.toml` (workspace root — restore `crates/gui` in `members`)

**Interfaces:**
- Consumes: `wasmffmpeg-core` (Tasks 1–5)
- Produces:
  - `backend::ProbeInfo { width: u32, height: u32, duration_secs: Option<f64> }`
  - `backend::BackendError { Spawn(String), ProbeFailed(String), Failed(String), Cancelled }` (`Debug + Clone + Display`, `Cancelled` Display = `"cancelled"`)
  - `backend::check_ffmpeg_available() -> Result<(), String>`
  - `backend::native::probe(input: &Path) -> Result<ProbeInfo, BackendError>`
  - `tail(stderr: &str) -> String` (crate-private helper in `native.rs`, also used by Task 7)

- [ ] **Step 1: Restore the workspace member and write the crate skeleton**

Workspace `Cargo.toml`: `members = ["crates/core", "crates/gui"]`.

`crates/gui/Cargo.toml`:
```toml
[package]
name = "wasmffmpeg-gui"
edition.workspace = true
license.workspace = true

[dependencies]
wasmffmpeg-core = { path = "../core" }
iced = { version = "0.14", features = ["sipper"] }
rfd = { version = "0.17", default-features = false, features = ["xdg-portal"] }
futures = "0.3"
sipper = "0.1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[dev-dependencies]
tempfile = "3"
```

`crates/gui/src/lib.rs`:
```rust
//! wasmffmpeg-gui: iced desktop app and execution backends.

pub mod app;
pub mod backend;

pub use app::run;
```

`crates/gui/src/main.rs`:
```rust
fn main() -> iced::Result {
    wasmffmpeg_gui::run()
}
```

`crates/gui/src/app.rs` (placeholder until Task 8 — keeps the crate compiling):
```rust
//! The iced application (implemented in Tasks 8–9).

/// Placeholder entry point.
pub fn run() -> iced::Result {
    Ok(())
}
```

- [ ] **Step 2: Write the failing integration test** — `crates/gui/tests/native_backend.rs`:

```rust
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
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p wasmffmpeg-gui`
Expected: FAIL — `backend` module does not exist.

- [ ] **Step 4: Implement `backend/mod.rs`**

```rust
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
```

- [ ] **Step 5: Implement `backend/native.rs`** (probe part; Task 7 appends `convert` and `unique_output_path`)

```rust
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
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p wasmffmpeg-gui`
Expected: PASS — 2 integration tests (plus placeholder app compiles). First dependency build takes a few minutes.

- [ ] **Step 7: Commit**

```bash
git add -A && git -c user.name=dev -c user.email=dev@local commit -m "feat(gui): crate skeleton, backend types, ffprobe probing"
```

---

### Task 7: Native backend convert (sipper) + unique output path

**Files:**
- Modify: `crates/gui/src/backend/native.rs` (append)
- Modify: `crates/gui/tests/native_backend.rs` (append)

**Interfaces:**
- Consumes: `ConversionJob`, `build_ffmpeg_args`, `parse_progress_line`, `ProgressEvent`, `fraction`, `candidate_name` (core); `tail` (Task 6)
- Produces:
  - `native::convert(job: ConversionJob, total_duration_secs: Option<f64>, cancel: Arc<AtomicBool>) -> impl sipper::Sipper<Result<PathBuf, BackendError>, f32>` — drives `Task::sip` in Task 9
  - `native::unique_output_path(dir: &Path, input: &Path, format: OutputFormat) -> PathBuf` — used by Task 9

- [ ] **Step 1: Write the failing tests** — append to `crates/gui/tests/native_backend.rs`:

```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p wasmffmpeg-gui`
Expected: FAIL — `convert` and `unique_output_path` do not exist.

- [ ] **Step 3: Implement** — append to `crates/gui/src/backend/native.rs`:

```rust
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
            "-hide_banner", "-nostdin", "-y", "-progress", "pipe:1", "-stats_period", "0.5",
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p wasmffmpeg-gui`
Expected: PASS — 7 integration tests. The VP9/AV1 encodes of the 2 s sample take a few seconds each; total run should stay under ~2 minutes.

- [ ] **Step 5: Commit**

```bash
git add -A && git -c user.name=dev -c user.email=dev@local commit -m "feat(gui): ffmpeg conversion worker with progress, cancel, collision-safe naming"
```

---

### Task 8: GUI shell — state, file intake, probing, queue view

**Files:**
- Modify: `crates/gui/src/app.rs` (replace placeholder)

**Interfaces:**
- Consumes: `backend::{native, check_ffmpeg_available, ProbeInfo, BackendError}`, core types from Tasks 1–5
- Produces (used by Task 9 which extends this same file):
  - `pub fn run() -> iced::Result`, `pub fn boot() -> (App, Task<Msg>)`, `fn update(&mut App, Msg) -> Task<Msg>`, `fn view(&App) -> Element<'_, Msg>`, `fn subscription(&App) -> Subscription<Msg>`
  - `App`, `Msg`, `Row`, `Status`, `Preset`, `JobId` as defined in the code below

- [ ] **Step 1: Write `app.rs`** — the full shell (conversion runner is stubbed at `start_next`, Task 9 fills it):

```rust
//! The iced application: state, messages, update, view, subscription.

use crate::backend::{self, ProbeInfo};
use iced::widget::{
    Column, button, checkbox, column, container, pick_list, progress_bar, row, scrollable, space,
    text, tooltip,
};
use iced::{Alignment, Element, Length, Subscription, Task, Theme, event, window};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use wasmffmpeg_core::{MediaKind, OutputFormat, ResizeSpec};

pub type JobId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Probing,
    Unsupported,
    Pending,
    Running,
    Done,
    Cancelled,
    Failed,
}

pub struct Row {
    pub id: JobId,
    pub input: PathBuf,
    pub kind: Option<MediaKind>,
    pub format: Option<OutputFormat>,
    pub status: Status,
    pub progress: f32,
    pub probe: Option<ProbeInfo>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Fhd,
    Hd,
    Uhd,
}

impl Preset {
    pub const ALL: &[Preset] = &[Preset::Fhd, Preset::Hd, Preset::Uhd];

    pub fn spec(self, allow_upscale: bool) -> ResizeSpec {
        let (width, height) = match self {
            Preset::Fhd => (1920, 1080),
            Preset::Hd => (1280, 720),
            Preset::Uhd => (3840, 2160),
        };
        ResizeSpec::new(width, height, allow_upscale)
    }
}

impl std::fmt::Display for Preset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Preset::Fhd => "1920×1080 (Full HD)",
            Preset::Hd => "1280×720 (HD)",
            Preset::Uhd => "3840×2160 (4K UHD)",
        })
    }
}

pub struct App {
    rows: Vec<Row>,
    next_id: JobId,
    out_dir: Option<PathBuf>,
    preset: Preset,
    allow_upscale: bool,
    ffmpeg_error: Option<String>,
    cancel_flags: HashMap<JobId, Arc<AtomicBool>>,
    running: Option<JobId>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    AddFiles,
    FilesAdded(Vec<PathBuf>),
    PickOutputDir,
    OutputDirPicked(Option<PathBuf>),
    FileDropped(PathBuf),
    Probed(JobId, Result<ProbeInfo, String>),
    SetRowFormat(JobId, OutputFormat),
    SetPreset(Preset),
    ToggleUpscale(bool),
    RemoveRow(JobId),
    ClearFinished,
    Start,
    JobProgress(JobId, f32),
    JobFinished(JobId, Result<PathBuf, backend::BackendError>),
    CancelJob(JobId),
    StopAll,
}

/// Application entry point called from `main`.
pub fn run() -> iced::Result {
    iced::application(boot, update, view)
        .subscription(subscription)
        .theme(Theme::Dark)
        .title("wasmffmpeg — media converter")
        .window_size([1100.0, 720.0])
        .centered()
        .run()
}

pub fn boot() -> (App, Task<Msg>) {
    let ffmpeg_error = backend::check_ffmpeg_available().err();
    (
        App {
            rows: Vec::new(),
            next_id: 0,
            out_dir: None,
            preset: Preset::Fhd,
            allow_upscale: false,
            ffmpeg_error,
            cancel_flags: HashMap::new(),
            running: None,
        },
        Task::none(),
    )
}

fn supported_extensions() -> Vec<&'static str> {
    wasmffmpeg_core::media::IMAGE_EXTENSIONS
        .iter()
        .chain(wasmffmpeg_core::media::VIDEO_EXTENSIONS)
        .copied()
        .collect()
}

fn update(app: &mut App, msg: Msg) -> Task<Msg> {
    match msg {
        Msg::AddFiles => Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .set_title("Add media files")
                    .add_filter("Media files", &supported_extensions())
                    .pick_files()
                    .await
                    .map(|handles| {
                        handles.iter().map(|h| h.path().to_path_buf()).collect()
                    })
                    .unwrap_or_default()
            },
            Msg::FilesAdded,
        ),
        Msg::FileDropped(path) => update(app, Msg::FilesAdded(vec![path])),
        Msg::FilesAdded(paths) => {
            let mut tasks = Vec::new();
            for path in paths {
                if app.rows.iter().any(|r| r.input == path) {
                    continue;
                }
                let kind = MediaKind::from_path(&path);
                let id = app.next_id;
                app.next_id += 1;
                let row = Row {
                    id,
                    input: path.clone(),
                    kind,
                    format: kind.map(OutputFormat::default_for),
                    status: if kind.is_some() { Status::Probing } else { Status::Unsupported },
                    progress: 0.0,
                    probe: None,
                    error: None,
                };
                app.rows.push(row);
                if kind.is_some() {
                    tasks.push(Task::perform(
                        {
                            let path = path.clone();
                            async move {
                                backend::native::probe(&path).map_err(|e| e.to_string())
                            }
                        },
                        move |result| Msg::Probed(id, result),
                    ));
                }
            }
            Task::batch(tasks)
        }
        Msg::Probed(id, result) => {
            if let Some(row) = app.rows.iter_mut().find(|r| r.id == id) {
                match result {
                    Ok(info) => {
                        row.probe = Some(info);
                        row.status = Status::Pending;
                    }
                    Err(e) => {
                        row.error = Some(e);
                        row.status = Status::Failed;
                    }
                }
            }
            Task::none()
        }
        Msg::PickOutputDir => Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .set_title("Choose output folder")
                    .pick_folder()
                    .await
                    .map(|h| h.path().to_path_buf())
            },
            Msg::OutputDirPicked,
        ),
        Msg::OutputDirPicked(dir) => {
            app.out_dir = dir;
            Task::none()
        }
        Msg::SetRowFormat(id, format) => {
            if let Some(row) = app.rows.iter_mut().find(|r| r.id == id) {
                row.format = Some(format);
            }
            Task::none()
        }
        Msg::SetPreset(preset) => {
            app.preset = preset;
            Task::none()
        }
        Msg::ToggleUpscale(allow) => {
            app.allow_upscale = allow;
            Task::none()
        }
        Msg::RemoveRow(id) => {
            if app.running != Some(id) {
                app.rows.retain(|r| r.id != id);
            }
            Task::none()
        }
        Msg::ClearFinished => {
            app.rows
                .retain(|r| !matches!(r.status, Status::Done | Status::Cancelled));
            Task::none()
        }
        Msg::Start => {
            if app.running.is_some() {
                return Task::none();
            }
            start_next(app)
        }
        Msg::JobProgress(id, progress) => {
            if let Some(row) = app.rows.iter_mut().find(|r| r.id == id) {
                row.progress = progress;
            }
            Task::none()
        }
        Msg::JobFinished(id, result) => {
            app.cancel_flags.remove(&id);
            if app.running == Some(id) {
                app.running = None;
            }
            if let Some(row) = app.rows.iter_mut().find(|r| r.id == id) {
                match result {
                    Ok(_) => {
                        row.status = Status::Done;
                        row.progress = 1.0;
                    }
                    Err(backend::BackendError::Cancelled) => {
                        row.status = Status::Cancelled;
                    }
                    Err(e) => {
                        row.status = Status::Failed;
                        row.error = Some(e.to_string());
                    }
                }
            }
            start_next(app)
        }
        Msg::CancelJob(id) => {
            if let Some(flag) = app.cancel_flags.get(&id) {
                flag.store(true, Ordering::Relaxed);
            }
            Task::none()
        }
        Msg::StopAll => {
            for flag in app.cancel_flags.values() {
                flag.store(true, Ordering::Relaxed);
            }
            for row in app.rows.iter_mut() {
                if row.status == Status::Pending {
                    row.status = Status::Cancelled;
                }
            }
            Task::none()
        }
    }
}

impl App {
    fn effective_out_dir(&self) -> Option<PathBuf> {
        self.out_dir.clone().or_else(|| {
            self.rows
                .iter()
                .find_map(|r| r.input.parent().map(|p| p.join("converted")))
        })
    }
}

/// Starts the next pending job. Filled in by Task 9; for now it only
/// validates that there is work, so the shell compiles and runs.
fn start_next(_app: &mut App) -> Task<Msg> {
    Task::none()
}

fn subscription(_app: &App) -> Subscription<Msg> {
    event::listen().filter_map(|event| match event {
        iced::Event::Window(window::Event::FileDropped(path)) => Some(Msg::FileDropped(path)),
        _ => None,
    })
}

fn view(app: &App) -> Element<'_, Msg> {
    let mut content = column![].spacing(12);

    if let Some(error) = &app.ffmpeg_error {
        content = content.push(
            container(text(error).size(14))
                .padding(12)
                .width(Length::Fill)
                .style(container::danger),
        );
    }

    let header = row![
        text("wasmffmpeg").size(24),
        space().width(Length::Fill),
        button("Add files").on_press(Msg::AddFiles),
        button("Output folder…").on_press(Msg::PickOutputDir),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let controls = row![
        text("Size:"),
        pick_list(Preset::ALL, Some(app.preset), Msg::SetPreset),
        checkbox(app.allow_upscale)
            .label("Allow upscale")
            .on_toggle(Msg::ToggleUpscale),
        space().width(Length::Fill),
        text(format!(
            "Output: {}",
            app.effective_out_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "—".into())
        ))
        .size(13),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let pending = app.rows.iter().filter(|r| r.status == Status::Pending).count();
    let mut start_button = button("Convert");
    if pending > 0 && app.ffmpeg_error.is_none() && app.running.is_none() {
        start_button = start_button.on_press(Msg::Start);
    }
    let mut stop_button = button("Stop all");
    if app.running.is_some() {
        stop_button = stop_button.on_press(Msg::StopAll);
    }
    let actions = row![
        start_button,
        stop_button,
        button("Clear finished").on_press(Msg::ClearFinished),
        text(format!(
            "{} file(s), {} ready",
            app.rows.len(),
            pending
        ))
        .size(13),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let list = app
        .rows
        .iter()
        .fold(Column::new().spacing(8), |col, row| col.push(view_row(row)));

    content = content.push(header).push(controls).push(actions).push(scrollable(list));
    container(content).padding(16).into()
}

fn view_row(row: &Row) -> Element<'_, Msg> {
    let id = row.id;
    let name = row
        .input
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?")
        .to_string();

    let kind_label = match row.kind {
        Some(MediaKind::Image) => "image",
        Some(MediaKind::Video) => "video",
        None => "unsupported",
    };

    let mut line = row![
        text(name).width(Length::FillPortion(3)),
        text(kind_label).width(Length::FillPortion(1)),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    if let (Some(kind), Some(format)) = (row.kind, row.format) {
        line = line.push(
            pick_list(OutputFormat::all_for(kind), Some(format), move |f| {
                Msg::SetRowFormat(id, f)
            })
            .width(Length::FillPortion(2)),
        );
    } else {
        line = line.push(text("—").width(Length::FillPortion(2)));
    }

    let status_widget: Element<'_, Msg> = match row.status {
        Status::Probing => text("Probing…").size(13).into(),
        Status::Unsupported => text("Unsupported type").size(13).into(),
        Status::Pending => {
            let dims = row
                .probe
                .map(|p| format!("{}×{}", p.width, p.height))
                .unwrap_or_default();
            text(format!("Ready {dims}")).size(13).into()
        }
        Status::Running if row.kind == Some(MediaKind::Image) => {
            text("Processing…").size(13).into()
        }
        Status::Running => progress_bar(0.0..=1.0, row.progress).into(),
        Status::Done => text("Done").size(13).into(),
        Status::Cancelled => text("Cancelled").size(13).into(),
        Status::Failed => tooltip(
            text("Failed").size(13),
            text(row.error.clone().unwrap_or_default()).size(12),
            tooltip::Position::Bottom,
        )
        .into(),
    };
    line = line.push(container(status_widget).width(Length::FillPortion(2)));

    if row.status == Status::Running {
        line = line.push(button("Cancel").on_press(Msg::CancelJob(id)));
    } else {
        line = line.push(button("Remove").on_press(Msg::RemoveRow(id)));
    }

    line.into()
}
```

- [ ] **Step 2: Build**

Run: `cargo build -p wasmffmpeg-gui`
Expected: compiles. Fix any drift from the iced 0.14 API (this file was written against the verified 0.14.0 sources; signatures used: `application(boot, update, view)`, `.subscription/.theme/.title/.window_size/.centered/.run`, `checkbox(bool).label(..).on_toggle(..)`, `pick_list(&[T], Option<T>, fn(T)->Msg)`, `progress_bar(range, f32)`, `event::listen().filter_map(..)`, `container::danger`).

- [ ] **Step 3: Add a unit test for `Preset::spec`** — append to `app.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_specs() {
        assert_eq!(Preset::Fhd.spec(false), ResizeSpec::new(1920, 1080, false));
        assert_eq!(Preset::Hd.spec(true), ResizeSpec::new(1280, 720, true));
        assert_eq!(Preset::Uhd.spec(false), ResizeSpec::new(3840, 2160, false));
    }
}
```

Run: `cargo test -p wasmffmpeg-gui`
Expected: PASS.

- [ ] **Step 4: Smoke-run the shell (if a display is available)**

Run: `cargo run -p wasmffmpeg-gui` (skip when `$DISPLAY` is unset; note it in the task result)
Expected: window opens; Add files lists probed dimensions; Convert button does nothing yet (runner lands in Task 9).

- [ ] **Step 5: Commit**

```bash
git add -A && git -c user.name=dev -c user.email=dev@local commit -m "feat(gui): app shell with file intake, probing, queue view"
```

---

### Task 9: Conversion runner (`start_next`) — sequential queue with progress + cancel

**Files:**
- Modify: `crates/gui/src/app.rs` (`start_next` function only)

**Interfaces:**
- Consumes: `native::convert(job, duration_secs, cancel_flag) -> impl sipper::Sipper<Result<PathBuf, BackendError>, f32>` and `native::unique_output_path(dir, input, format)` (Task 7); `ConversionJob` (Task 3); `App`/`Msg`/`Status`/`Row` (Task 8)
- Produces: the working sequential queue; no new public interfaces

- [ ] **Step 1: Replace the `start_next` stub in `app.rs`**

```rust
/// Starts the next pending job, if any. Rows whose output directory cannot
/// be created fail immediately and the queue moves on. Exactly one ffmpeg
/// runs at a time.
fn start_next(app: &mut App) -> Task<Msg> {
    let Some(out_dir) = app.effective_out_dir() else {
        return Task::none();
    };
    let Some(index) = app
        .rows
        .iter()
        .position(|r| r.status == Status::Pending && r.probe.is_some())
    else {
        app.running = None;
        return Task::none();
    };

    let row = &mut app.rows[index];
    let id = row.id;
    let probe = row.probe.expect("checked above");
    let format = row.format.expect("pending rows always have a format");

    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        row.status = Status::Failed;
        row.error = Some(format!("cannot create output folder: {e}"));
        return start_next(app);
    }

    let output = backend::native::unique_output_path(&out_dir, &row.input, format);
    row.status = Status::Running;
    row.progress = 0.0;

    let flag = Arc::new(AtomicBool::new(false));
    app.cancel_flags.insert(id, flag.clone());
    app.running = Some(id);

    let job = wasmffmpeg_core::ConversionJob {
        input: row.input.clone(),
        output,
        format,
        input_width: probe.width,
        input_height: probe.height,
        resize: app.preset.spec(app.allow_upscale),
    };
    let duration = probe.duration_secs;
    let sipper = backend::native::convert(job, duration, flag);
    Task::sip(
        sipper,
        move |progress| Msg::JobProgress(id, progress),
        move |output| Msg::JobFinished(id, output),
    )
}
```

- [ ] **Step 2: Build and run the full test suite**

Run: `cargo test --workspace`
Expected: PASS — all core unit tests, app unit test, and backend integration tests.

- [ ] **Step 3: Manual end-to-end check (if a display is available)**

```bash
mkdir -p /tmp/wasmffmpeg-smoke && cd /tmp/wasmffmpeg-smoke
ffmpeg -f lavfi -i testsrc=size=2560x1440 -frames:v 1 -y big.png
ffmpeg -f lavfi -i testsrc=size=640x360:duration=3:rate=15 -f lavfi -i sine=frequency=440:duration=3 -c:v libx264 -c:a aac -shortest -y clip.mp4
cargo run --manifest-path <repo>/Cargo.toml -p wasmffmpeg-gui
```
In the UI: add both files → set image to WebP, video to WebM / AV1 → Convert → watch progress → outputs land in `/tmp/wasmffmpeg-smoke/converted/`. Verify with `ffprobe` that the image is 1920×1080 and the video 640×360 (no upscale).

- [ ] **Step 4: Commit**

```bash
git add -A && git -c user.name=dev -c user.email=dev@local commit -m "feat(gui): sequential conversion runner with progress and cancel"
```

---

### Task 10: Final verification + README

**Files:**
- Create: `README.md`

**Interfaces:**
- Consumes: everything
- Produces: green workspace; README with run/test/web-phase instructions

- [ ] **Step 1: Full verification gate**

Run, in order:
```bash
cargo fmt --all -- --check || cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check -p wasmffmpeg-core --target wasm32-unknown-unknown
```
Expected: fmt clean (or formatted), clippy with zero warnings, all tests pass, core compiles for wasm32.

- [ ] **Step 2: Write `README.md`**

```markdown
# wasmffmpeg

Desktop media converter built with Rust, [iced](https://iced.rs) 0.14 and
FFmpeg. Converts and resizes images (JPEG, PNG, WebP, AVIF) and videos
(MP4 H.264/AV1, WebM VP9/AV1), one file or in bulk. Everything is scaled to
fit inside **1920×1080** (configurable) while preserving aspect ratio;
upscaling is off by default.

## Requirements

- Rust 1.98+ (`rustup`)
- FFmpeg 6+ on PATH: `sudo apt install ffmpeg`

## Run

```bash
cargo run -p wasmffmpeg-gui
```

## Test

```bash
cargo test --workspace
```

(Integration tests use the system ffmpeg and skip automatically when it is
not installed.)

## Layout

- `crates/core` — pure Rust conversion logic: media types, format catalog,
  aspect-ratio resize math, ffmpeg argument builder, progress parsing,
  output name generation. No I/O; compiles to `wasm32-unknown-unknown`
  (`cargo check -p wasmffmpeg-core --target wasm32-unknown-unknown`).
- `crates/gui` — iced 0.14 app plus the native backend that runs the
  system `ffmpeg`/`ffprobe` subprocesses.

## WebAssembly (phase 2)

The core is already wasm-safe. The browser version will reuse every
conversion command built by `wasmffmpeg-core` and execute them with
[ffmpeg.wasm](https://github.com/ffmpegwasm/ffmpeg.wasm)
(`@ffmpeg/ffmpeg` 0.12 + `@ffmpeg/core-mt`) through a small wasm-bindgen
JS glue layer, behind `cfg(target_arch = "wasm32")` in `crates/gui/src/backend/`.
Serving the multithreaded core requires COOP/COEP response headers.
```

- [ ] **Step 3: Commit**

```bash
git add -A && git -c user.name=dev -c user.email=dev@local commit -m "docs: README with run/test/wasm-phase instructions"
```

---

## Self-review notes

- Spec coverage: media detection ✓ (T1), formats + recipes ✓ (T1/T3), resize semantics ✓ (T2), progress ✓ (T4), naming/no-overwrite ✓ (T5/T7), probing ✓ (T6), worker/cancel ✓ (T7), GUI intake/queue/options ✓ (T8), sequential runner ✓ (T9), error surfaces (startup ffmpeg check, per-row errors, out-dir failure) ✓ (T6/T8/T9), testing ✓ (all tasks), wasm-readiness gate ✓ (T5/T10), README ✓ (T10).
- Type consistency: `BackendError` is `Clone + PartialEq` so `Msg::JobFinished(JobId, Result<PathBuf, BackendError>)` stays `Clone` and the cancel test can compare against `"cancelled".to_string()` via `Display`. `Sipper<Output, Progress>` generic order is output-first, matching sipper 0.1 (`pub trait Sipper<Output, Progress = Output>`).
