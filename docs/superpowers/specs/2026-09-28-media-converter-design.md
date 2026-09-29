# Media Converter — Design Spec

**Date:** 2026-09-28
**Status:** Approved by user (2026-09-28)
**Working title:** `wasmffmpeg`

## Summary

A desktop GUI application (Rust + iced 0.14) for converting and resizing images
and videos, individually or in bulk. All media work goes through FFmpeg: the
system `ffmpeg`/`ffprobe` binaries on desktop (subprocess), and ffmpeg.wasm via a
JavaScript bridge when the app is later compiled to WebAssembly for the browser.
The architecture is WASM-ready from day one: all conversion logic lives in a
pure-Rust, I/O-free `core` crate that compiles to `wasm32-unknown-unknown`.

## Goals

- Resize images and videos to fit inside **1920×1080** (default), always
  preserving the original aspect ratio.
- Image conversion, single or bulk: **JPEG, PNG, WebP, AVIF** (input and output).
- Video conversion, single or bulk: **MP4/H.264**, **MP4/AV1**, **WebM/VP9**,
  **WebM/AV1** (input: any container/codec FFmpeg can decode).
- Bulk = a queue of files processed sequentially, with per-file progress and
  per-file success/failure status.
- Keep the codebase compilable to `wasm32-unknown-unknown` (core crate verified
  in v1; full web harness in a later phase).

## Non-goals (v1)

- Full web/WASM harness (Trunk setup, JS glue for ffmpeg.wasm, COOP/COEP
  headers). v1 delivers the native app plus a wasm-safe core; the web backend
  lands in phase 2.
- Video trimming, filters, watermarks, audio-only conversion, GIF.
- Hardware-accelerated encoding flags, parallel queue processing, custom
  free-form FFmpeg arguments.
- Padding/letterboxing to exactly 1920×1080 (may be added later; v1 only fits
  inside the box).

## Key technical decision

ffmpeg.wasm is a **JavaScript** library (`@ffmpeg/ffmpeg` 0.12.15 +
`@ffmpeg/core-mt` 0.12.10), not a Rust crate. Rust→WASM code cannot link to it;
it must call it through JS interop (wasm-bindgen). There is no practical
pure-Rust FFmpeg that compiles to WASM. Therefore:

- **All conversion logic is expressed as FFmpeg command lines** built by the
  pure-Rust `core` crate. This is the single source of truth, shared by both
  execution backends.
- **Native backend** spawns the system `ffmpeg`/`ffprobe` binaries
  (ffmpeg 6.1.1 confirmed installed, with libx264, libaom-av1, libsvtav1,
  librav1e, libvpx-vp9, libwebp).
- **WASM backend (phase 2)** passes the same argument list to ffmpeg.wasm
  through a thin JS glue layer (~100 lines) via wasm-bindgen.

Chosen over: (B) pure-Rust image crates + FFmpeg only for video — rejected
because pure-Rust AVIF *decoding* is immature and it doubles the pipelines;
(C) `ffmpeg-next` C bindings — rejected because they can never target WASM.

Known limitation: in the browser, ffmpeg.wasm encodes AV1 with libaom only (no
SVT-AV1) and runs without hardware acceleration; large AV1 encodes will be
slow. Desktop is the primary target for heavy video work.

## Repository layout

Cargo workspace:

```
wasmffmpeg/
├── Cargo.toml              # workspace root
├── crates/
│   ├── core/               # wasmffmpeg-core: pure Rust, no I/O, wasm-safe
│   │   └── src/lib.rs (+ modules)
│   └── gui/                # wasmffmpeg-gui: iced 0.14 app + backends
│       └── src/main.rs (+ modules)
├── docs/superpowers/specs/ # this spec
└── web/                    # phase 2: index.html, JS glue, Trunk config
```

### `core` crate (pure, no I/O, no std::process/fs)

Responsibilities:

- Media model: `MediaKind { Image, Video }`, detected from file extension.
- Output format catalog:
  - `ImageFormat { Jpeg, Png, WebP, Avif }`
  - `VideoFormat { Mp4H264, Mp4Av1, WebMVp9, WebMAv1 }`
- `ResizeSpec { width: u32, height: u32, allow_upscale: bool }` — default
  `1920×1080`, `allow_upscale: false`.
- `ConversionJob { input, output, kind, format, resize }` and
  `build_ffmpeg_args(&ConversionJob) -> Vec<String>` — the complete argument
  list (excluding the ffmpeg executable name).
- `candidate_name(input, format, suffix) -> String` — pure output filename
  generation with the correct extension and an optional `_1`, `_2`, … suffix.
  The backend loops over suffixes and performs the actual
  filesystem-collision check, so existing files are never overwritten.
- Progress parsing: pure function `parse_progress(line) -> Option<Progress>`
  for ffmpeg `-progress pipe:1` key=value output, plus
  `Progress::fraction(total_duration)`.
- `probe_duration` helper: builds the ffprobe argument list (parsing the JSON
  output lives in the backend; core only owns argument construction to stay
  I/O-free).

Dependencies: none beyond `std` (serde is **not** needed in core; ffprobe JSON
parsing in the backend uses a minimal extraction or `serde_json` in the gui
crate only).

### Native backend (in `gui` crate, module `backend::native`)

- Locates `ffmpeg`/`ffprobe` on `PATH` at startup; reports a clear error if
  missing.
- Probing: every input is probed once with
  `ffprobe -v error -select_streams v:0 -show_entries stream=width,height -show_entries format=duration -of json <input>`.
  Width/height feed the no-upscale clamp (images and videos); duration feeds
  progress computation (videos only).
- Execution: spawn `ffmpeg -hide_banner -nostdin -y -progress pipe:1
  -stats_period 0.5 <args>`, stream stdout line-by-line, forward `Progress`
  updates to the GUI through an iced `Subscription` (channel-based stream).
  Capture the last ~4 KiB of stderr for error display.
- Cancellation: kill the child process; remove a partial output file.
- Sequential queue worker: one ffmpeg at a time (ffmpeg already uses all
  cores; parallel jobs would thrash). A failed file does not stop the queue.
- The wasm backend (phase 2) will implement the same interface behind
  `cfg(target_arch = "wasm32")`.

### GUI (`gui` crate, iced 0.14)

Single window, Elm-style `Model/Msg/update/view`:

- **Input**: "Add files" button (rfd multi-select) and drag-and-drop. Each file
  is classified by extension into image/video; unknown extensions are listed
  as unsupported and excluded from conversion.
- **Per-job settings**: output format `pick_list` filtered by media kind
  (image formats for images, video formats for videos); a global default plus
  per-row override. Global resize preset `pick_list`: 1920×1080 (default),
  1280×720, 3840×2160; "Allow upscale" checkbox (default off).
- **Output**: output folder picker (default: `<first input's dir>/converted/`,
  created if missing); auto-rename on collision, never overwrite.
- **Queue**: rows with file name, detected kind, target format, a progress
  bar, and status: Pending / Running (%) / Done / Failed (error tooltip or
  expandable stderr tail). Cancel button per running row and a global stop.
- **Run**: "Convert" starts the sequential worker; UI stays responsive
  (ffmpeg runs in a subscription stream, not on the UI thread).

## Conversion recipes (FFmpeg arguments)

Common: `-hide_banner -nostdin -y -progress pipe:1` prepended by the backend;
`build_ffmpeg_args` returns everything after the executable name, starting
with `-i <input>`.

Resize filter (videos): `scale=w=W:h=H:force_original_aspect_ratio=decrease:force_divisible_by=2`
When `allow_upscale` is false, clamp the requested box to the input dimensions
first (the backend supplies probed width/height; core computes
`effective_box(input_w, input_h, spec)`) so small inputs pass through at
native size. Images use the same filter minus `force_divisible_by=2`.

Images (`-frames:v 1` implied by single-image input):

| Format | Arguments (after input) |
|--------|-------------------------|
| JPEG   | `-vf scale=… -q:v 2 out.jpg` |
| PNG    | `-vf scale=… out.png` |
| WebP   | `-vf scale=… -c:v libwebp -quality 85 out.webp` |
| AVIF   | `-vf scale=… -c:v libaom-av1 -still-picture 1 -crf 30 -b:v 0 out.avif` |

Videos (audio always re-encoded for predictable results):

| Format | Arguments (after input) |
|--------|-------------------------|
| MP4/H.264 | `-vf scale=… -c:v libx264 -preset medium -crf 20 -pix_fmt yuv420p -c:a aac -b:a 160k -movflags +faststart out.mp4` |
| MP4/AV1   | `-vf scale=… -c:v libsvtav1 -preset 6 -crf 30 -pix_fmt yuv420p -c:a aac -b:a 160k -movflags +faststart out.mp4` |
| WebM/VP9  | `-vf scale=… -c:v libvpx-vp9 -crf 32 -b:v 0 -row-mt 1 -pix_fmt yuv420p -c:a libopus -b:a 128k out.webm` |
| WebM/AV1  | `-vf scale=… -c:v libsvtav1 -preset 6 -crf 32 -pix_fmt yuv420p -c:a libopus -b:a 128k out.webm` |

Inputs with no audio stream: the `-c:a` options are harmlessly ignored by
FFmpeg (no audio mapping error) because no audio stream exists to encode.

Media kind detection is **extension-based** (predictable, no probing needed
for classification):

- Images: `jpg jpeg png webp avif` (also decode-side: `bmp gif tiff tif` are
  accepted as inputs and classified as images).
- Videos: `mp4 m4v mov mkv webm avi mpg mpeg ts m2ts 3gp flv wmv vob ogv`.
- Anything else: unsupported row in the UI.

## Error handling

- Missing ffmpeg/ffprobe at startup → modal-style error panel with install
  instructions; conversion controls disabled.
- Per-file failure → row marked Failed with the tail of ffmpeg stderr;
  queue continues.
- Probe failure (corrupt/undreadable input) → row marked Failed before
  conversion starts, with the ffprobe error.
- Output directory not writable → surfaced when the job starts, row fails
  with a clear message; remaining jobs continue.
- Core functions are total: argument building cannot fail; path/name
  generation is pure; all fallible I/O lives in the backend and returns
  structured `BackendError` values.

## Testing

- `core` unit tests (TDD):
  - aspect-ratio fit math: landscape, portrait, exact match, upscale
    allowed/disallowed, odd input dimensions, `force_divisible_by=2` effect;
  - argument builder: exact argument lists per format, resize on/off,
    upscale clamp on/off;
  - progress parser: `out_time_us`/`out_time_ms` lines, fraction computation,
    malformed lines ignored;
  - output name generation: extension mapping, `_1`/`_2` suffixing.
- Integration test (in `gui`, gated on ffmpeg presence — skips with a message
  if `ffmpeg` is not on PATH): generate a sample PNG and a 1-second
  `testsrc` video with `ffmpeg -f lavfi`, run real conversions through the
  native backend for each output format, and assert the output files exist,
  are non-empty, and (for videos) probe at the expected dimensions.
- Manual verification script of the real user flow: launch the app, convert a
  mixed batch.

## WASM readiness contract

- `core` has zero dependencies outside `std` and performs no I/O;
  `cargo check -p wasmffmpeg-core --target wasm32-unknown-unknown` must pass
  in v1 and stay green.
- `gui` isolates all native-only code (subprocess, paths, rfd native APIs) in
  `backend::native` behind `#[cfg(not(target_arch = "wasm32"))]`; the
  conversion interface the GUI talks to is target-agnostic.
- Phase 2 (not this spec): Trunk harness, `webgl` iced feature, JS glue to
  `@ffmpeg/ffmpeg` 0.12.15 with `@ffmpeg/core-mt` 0.12.10, COOP/COEP headers
  (`Cross-Origin-Opener-Policy: same-origin`,
  `Cross-Origin-Embedder-Policy: require-corp`) for SharedArrayBuffer.

## Milestones

1. Workspace scaffold + `core` (TDD) + unit tests green.
2. Native backend + integration tests green.
3. iced GUI wired to backend; manual end-to-end run of a mixed batch.
4. `cargo clippy` clean; wasm32 check of `core` green.
5. (Phase 2, separate spec) web harness + ffmpeg.wasm backend.
