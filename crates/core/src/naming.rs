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
