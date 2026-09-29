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
    let (box_w, box_h) = resize::effective_box(job.input_width, job.input_height, &job.resize);
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

    #[test]
    fn jpeg_args() {
        let args = job(OutputFormat::Image(ImageFormat::Jpeg), 3840, 2160, false);
        assert_eq!(
            build_ffmpeg_args(&args),
            vec![
                "-i",
                "/in/src.mkv",
                "-vf",
                "scale=w=1920:h=1080:force_original_aspect_ratio=decrease",
                "-frames:v",
                "1",
                "-q:v",
                "2",
                "/out/dst.bin",
            ]
        );
    }

    #[test]
    fn png_args_have_no_codec_flags() {
        let args = job(OutputFormat::Image(ImageFormat::Png), 3840, 2160, false);
        assert_eq!(
            build_ffmpeg_args(&args),
            vec![
                "-i",
                "/in/src.mkv",
                "-vf",
                "scale=w=1920:h=1080:force_original_aspect_ratio=decrease",
                "-frames:v",
                "1",
                "/out/dst.bin",
            ]
        );
    }

    #[test]
    fn webp_args() {
        let args = job(OutputFormat::Image(ImageFormat::WebP), 3840, 2160, false);
        assert_eq!(
            build_ffmpeg_args(&args),
            vec![
                "-i",
                "/in/src.mkv",
                "-vf",
                "scale=w=1920:h=1080:force_original_aspect_ratio=decrease",
                "-frames:v",
                "1",
                "-c:v",
                "libwebp",
                "-quality",
                "85",
                "/out/dst.bin",
            ]
        );
    }

    #[test]
    fn avif_args() {
        let args = job(OutputFormat::Image(ImageFormat::Avif), 3840, 2160, false);
        assert_eq!(
            build_ffmpeg_args(&args),
            vec![
                "-i",
                "/in/src.mkv",
                "-vf",
                "scale=w=1920:h=1080:force_original_aspect_ratio=decrease",
                "-frames:v",
                "1",
                "-c:v",
                "libaom-av1",
                "-still-picture",
                "1",
                "-crf",
                "30",
                "-b:v",
                "0",
                "/out/dst.bin",
            ]
        );
    }

    #[test]
    fn h264_args() {
        let args = job(OutputFormat::Video(VideoFormat::Mp4H264), 3840, 2160, false);
        assert_eq!(
            build_ffmpeg_args(&args),
            vec![
                "-i",
                "/in/src.mkv",
                "-vf",
                "scale=w=1920:h=1080:force_original_aspect_ratio=decrease:force_divisible_by=2",
                "-c:v",
                "libx264",
                "-preset",
                "medium",
                "-crf",
                "20",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-b:a",
                "160k",
                "-movflags",
                "+faststart",
                "/out/dst.bin",
            ]
        );
    }

    #[test]
    fn mp4_av1_args() {
        let args = job(OutputFormat::Video(VideoFormat::Mp4Av1), 3840, 2160, false);
        assert_eq!(
            build_ffmpeg_args(&args),
            vec![
                "-i",
                "/in/src.mkv",
                "-vf",
                "scale=w=1920:h=1080:force_original_aspect_ratio=decrease:force_divisible_by=2",
                "-c:v",
                "libsvtav1",
                "-preset",
                "6",
                "-crf",
                "30",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-b:a",
                "160k",
                "-movflags",
                "+faststart",
                "/out/dst.bin",
            ]
        );
    }

    #[test]
    fn webm_vp9_args() {
        let args = job(OutputFormat::Video(VideoFormat::WebMVp9), 3840, 2160, false);
        assert_eq!(
            build_ffmpeg_args(&args),
            vec![
                "-i",
                "/in/src.mkv",
                "-vf",
                "scale=w=1920:h=1080:force_original_aspect_ratio=decrease:force_divisible_by=2",
                "-c:v",
                "libvpx-vp9",
                "-crf",
                "32",
                "-b:v",
                "0",
                "-row-mt",
                "1",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "libopus",
                "-b:a",
                "128k",
                "/out/dst.bin",
            ]
        );
    }

    #[test]
    fn webm_av1_args() {
        let args = job(OutputFormat::Video(VideoFormat::WebMAv1), 3840, 2160, false);
        assert_eq!(
            build_ffmpeg_args(&args),
            vec![
                "-i",
                "/in/src.mkv",
                "-vf",
                "scale=w=1920:h=1080:force_original_aspect_ratio=decrease:force_divisible_by=2",
                "-c:v",
                "libsvtav1",
                "-preset",
                "6",
                "-crf",
                "32",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "libopus",
                "-b:a",
                "128k",
                "/out/dst.bin",
            ]
        );
    }

    #[test]
    fn no_upscale_clamps_box_into_filter() {
        let args = job(OutputFormat::Video(VideoFormat::Mp4H264), 1000, 500, false);
        assert!(
            build_ffmpeg_args(&args).contains(
                &"scale=w=1000:h=500:force_original_aspect_ratio=decrease:force_divisible_by=2"
                    .to_string()
            )
        );
    }

    #[test]
    fn allow_upscale_uses_full_box() {
        let args = job(OutputFormat::Video(VideoFormat::Mp4H264), 1000, 500, true);
        assert!(
            build_ffmpeg_args(&args).contains(
                &"scale=w=1920:h=1080:force_original_aspect_ratio=decrease:force_divisible_by=2"
                    .to_string()
            )
        );
    }
}
