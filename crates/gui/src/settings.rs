//! User settings persisted between restarts (native only; the wasm backend
//! will store them in browser storage in phase 2).
//!
//! Stored as JSON at `$XDG_CONFIG_HOME/wasmffmpeg/settings.json`
//! (`~/.config/wasmffmpeg/settings.json` on most Linux desktops,
//! `%APPDATA%\wasmffmpeg\settings.json` on Windows).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use wasmffmpeg_core::{ImageFormat, ResizeSpec, VideoFormat};

/// Output size preset.
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

/// Everything remembered between restarts.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// User-chosen output folder. `None` means "same folder as each input".
    pub out_dir: Option<PathBuf>,
    pub image_format: ImageFormat,
    pub video_format: VideoFormat,
    pub preset: Preset,
    pub allow_upscale: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            out_dir: None,
            image_format: ImageFormat::Jpeg,
            video_format: VideoFormat::Mp4H264,
            preset: Preset::Fhd,
            allow_upscale: false,
        }
    }
}

/// On-disk representation with plain, forward-compatible fields. Unknown
/// enum tags fall back to defaults so older/newer files never crash us.
#[derive(Serialize, Deserialize)]
struct SettingsFile {
    out_dir: Option<PathBuf>,
    image_format: String,
    video_format: String,
    preset: String,
    allow_upscale: bool,
}

fn image_format_tag(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Jpeg => "jpeg",
        ImageFormat::Png => "png",
        ImageFormat::WebP => "webp",
        ImageFormat::Avif => "avif",
    }
}

fn image_format_from_tag(tag: &str) -> Option<ImageFormat> {
    match tag {
        "jpeg" => Some(ImageFormat::Jpeg),
        "png" => Some(ImageFormat::Png),
        "webp" => Some(ImageFormat::WebP),
        "avif" => Some(ImageFormat::Avif),
        _ => None,
    }
}

fn video_format_tag(format: VideoFormat) -> &'static str {
    match format {
        VideoFormat::Mp4H264 => "mp4-h264",
        VideoFormat::Mp4Av1 => "mp4-av1",
        VideoFormat::WebMVp9 => "webm-vp9",
        VideoFormat::WebMAv1 => "webm-av1",
    }
}

fn video_format_from_tag(tag: &str) -> Option<VideoFormat> {
    match tag {
        "mp4-h264" => Some(VideoFormat::Mp4H264),
        "mp4-av1" => Some(VideoFormat::Mp4Av1),
        "webm-vp9" => Some(VideoFormat::WebMVp9),
        "webm-av1" => Some(VideoFormat::WebMAv1),
        _ => None,
    }
}

fn preset_tag(preset: Preset) -> &'static str {
    match preset {
        Preset::Fhd => "fhd",
        Preset::Hd => "hd",
        Preset::Uhd => "uhd",
    }
}

fn preset_from_tag(tag: &str) -> Option<Preset> {
    match tag {
        "fhd" => Some(Preset::Fhd),
        "hd" => Some(Preset::Hd),
        "uhd" => Some(Preset::Uhd),
        _ => None,
    }
}

impl Settings {
    fn to_file(&self) -> SettingsFile {
        SettingsFile {
            out_dir: self.out_dir.clone(),
            image_format: image_format_tag(self.image_format).to_string(),
            video_format: video_format_tag(self.video_format).to_string(),
            preset: preset_tag(self.preset).to_string(),
            allow_upscale: self.allow_upscale,
        }
    }

    fn from_file(file: SettingsFile) -> Settings {
        let defaults = Settings::default();
        Settings {
            out_dir: file.out_dir,
            image_format: image_format_from_tag(&file.image_format)
                .unwrap_or(defaults.image_format),
            video_format: video_format_from_tag(&file.video_format)
                .unwrap_or(defaults.video_format),
            preset: preset_from_tag(&file.preset).unwrap_or(defaults.preset),
            allow_upscale: file.allow_upscale,
        }
    }
}

/// The platform settings file location.
fn settings_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("wasmffmpeg").join("settings.json")
}

/// Loads settings from `path`; any problem yields the defaults.
pub fn load_from(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|json| serde_json::from_str::<SettingsFile>(&json).ok())
        .map(Settings::from_file)
        .unwrap_or_default()
}

/// Writes settings to `path`, creating parent directories as needed.
pub fn save_to(path: &Path, settings: &Settings) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&settings.to_file())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, json)
}

/// Loads the user's settings (defaults on any problem).
pub fn load() -> Settings {
    load_from(&settings_path())
}

/// Persists the user's settings. Errors are logged, never fatal: a lost
/// preference must not break a conversion session.
pub fn save(settings: &Settings) {
    if let Err(e) = save_to(&settings_path(), settings) {
        eprintln!("wasmffmpeg: could not save settings: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_specs() {
        assert_eq!(Preset::Fhd.spec(false), ResizeSpec::new(1920, 1080, false));
        assert_eq!(Preset::Hd.spec(true), ResizeSpec::new(1280, 720, true));
        assert_eq!(Preset::Uhd.spec(false), ResizeSpec::new(3840, 2160, false));
    }

    #[test]
    fn defaults_are_sensible() {
        let settings = Settings::default();
        assert_eq!(settings.out_dir, None);
        assert_eq!(settings.image_format, ImageFormat::Jpeg);
        assert_eq!(settings.video_format, VideoFormat::Mp4H264);
        assert_eq!(settings.preset, Preset::Fhd);
        assert!(!settings.allow_upscale);
    }

    #[test]
    fn round_trip_through_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let settings = Settings {
            out_dir: Some(std::path::PathBuf::from("/some/where")),
            image_format: ImageFormat::WebP,
            video_format: VideoFormat::WebMAv1,
            preset: Preset::Uhd,
            allow_upscale: true,
        };
        save_to(&path, &settings).unwrap();
        assert_eq!(load_from(&path), settings);
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            load_from(&dir.path().join("nope.json")),
            Settings::default()
        );
    }

    #[test]
    fn corrupt_json_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, b"{not json").unwrap();
        assert_eq!(load_from(&path), Settings::default());
    }

    #[test]
    fn unknown_format_tags_fall_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{"out_dir":null,"image_format":"tiff","video_format":"prores","preset":"8k","allow_upscale":true}"#,
        )
        .unwrap();
        let settings = load_from(&path);
        assert_eq!(settings.image_format, ImageFormat::Jpeg);
        assert_eq!(settings.video_format, VideoFormat::Mp4H264);
        assert_eq!(settings.preset, Preset::Fhd);
        assert!(settings.allow_upscale);
    }
}
