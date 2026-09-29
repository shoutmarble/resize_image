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
