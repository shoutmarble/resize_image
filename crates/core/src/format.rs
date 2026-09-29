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
    pub const ALL: &[VideoFormat] = &[Self::Mp4H264, Self::Mp4Av1, Self::WebMVp9, Self::WebMAv1];

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
        assert!(
            OutputFormat::all_for(MediaKind::Image)
                .iter()
                .all(|f| f.kind() == MediaKind::Image)
        );
        assert!(
            OutputFormat::all_for(MediaKind::Video)
                .iter()
                .all(|f| f.kind() == MediaKind::Video)
        );
    }

    #[test]
    fn defaults() {
        assert_eq!(
            OutputFormat::default_for(MediaKind::Image),
            OutputFormat::Image(ImageFormat::Jpeg)
        );
        assert_eq!(
            OutputFormat::default_for(MediaKind::Video),
            OutputFormat::Video(VideoFormat::Mp4H264)
        );
    }

    #[test]
    fn display_labels() {
        assert_eq!(ImageFormat::WebP.to_string(), "WebP");
        assert_eq!(VideoFormat::Mp4H264.to_string(), "MP4 / H.264");
        assert_eq!(VideoFormat::WebMAv1.to_string(), "WebM / AV1");
        assert_eq!(OutputFormat::Image(ImageFormat::Avif).to_string(), "AVIF");
    }
}
