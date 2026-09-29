//! Output file name generation.
//!
//! Converted files are named `RS-<yyyy-MM-dd>--<HH-mm-ss>-<name>.<ext>` —
//! `RS` marks the file as resized, the timestamp is the local time the
//! conversion started (supplied by the caller; this crate stays clock-free),
//! and `<name>` is the first seven characters of the original file stem.

use crate::format::OutputFormat;
use std::path::Path;

/// Generates the output file name (no directory component) for an input
/// file, target format, and a pre-formatted timestamp like
/// `"2026-09-28--14-30-05"`. `suffix` appends `_N` before the extension for
/// collision avoidance; the caller performs the actual filesystem check.
pub fn resized_name(
    input: &Path,
    format: OutputFormat,
    timestamp: &str,
    suffix: Option<u32>,
) -> String {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("file");
    let short: String = stem.chars().take(7).collect();
    let suffix = suffix.map(|n| format!("_{n}")).unwrap_or_default();
    format!("RS-{timestamp}-{short}{suffix}.{}", format.extension())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{ImageFormat, OutputFormat, VideoFormat};
    use std::path::Path;

    const TS: &str = "2026-09-28--14-30-05";

    #[test]
    fn resized_pattern_with_first_seven_chars_of_stem() {
        assert_eq!(
            resized_name(
                Path::new("/a/photoofamily.png"),
                OutputFormat::Image(ImageFormat::Jpeg),
                TS,
                None
            ),
            "RS-2026-09-28--14-30-05-photoof.jpg"
        );
    }

    #[test]
    fn stem_of_exactly_seven_chars_stays_whole() {
        assert_eq!(
            resized_name(
                Path::new("photoof.png"),
                OutputFormat::Image(ImageFormat::WebP),
                TS,
                None
            ),
            "RS-2026-09-28--14-30-05-photoof.webp"
        );
    }

    #[test]
    fn short_stem_used_whole() {
        assert_eq!(
            resized_name(
                Path::new("ab.png"),
                OutputFormat::Image(ImageFormat::Png),
                TS,
                None
            ),
            "RS-2026-09-28--14-30-05-ab.png"
        );
    }

    #[test]
    fn first_seven_counts_chars_not_bytes() {
        assert_eq!(
            resized_name(
                Path::new("héllo_world.png"),
                OutputFormat::Image(ImageFormat::Jpeg),
                TS,
                None
            ),
            "RS-2026-09-28--14-30-05-héllo_w.jpg"
        );
    }

    #[test]
    fn suffix_goes_before_extension() {
        assert_eq!(
            resized_name(
                Path::new("photo.png"),
                OutputFormat::Image(ImageFormat::Jpeg),
                TS,
                Some(3)
            ),
            "RS-2026-09-28--14-30-05-photo_3.jpg"
        );
    }

    #[test]
    fn video_formats_get_their_extension() {
        assert_eq!(
            resized_name(
                Path::new("holiday_clip.mov"),
                OutputFormat::Video(VideoFormat::WebMAv1),
                TS,
                None
            ),
            "RS-2026-09-28--14-30-05-holiday.webm"
        );
        assert_eq!(
            resized_name(
                Path::new("holiday_clip.mov"),
                OutputFormat::Video(VideoFormat::Mp4H264),
                TS,
                None
            ),
            "RS-2026-09-28--14-30-05-holiday.mp4"
        );
    }

    #[test]
    fn falls_back_when_no_stem() {
        assert_eq!(
            resized_name(
                Path::new("/"),
                OutputFormat::Image(ImageFormat::Jpeg),
                TS,
                None
            ),
            "RS-2026-09-28--14-30-05-file.jpg"
        );
    }
}
