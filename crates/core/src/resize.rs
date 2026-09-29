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
        Self {
            width,
            height,
            allow_upscale,
        }
    }
}

impl Default for ResizeSpec {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            allow_upscale: false,
        }
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
    let mut filter = format!("scale=w={box_w}:h={box_h}:force_original_aspect_ratio=decrease");
    if even_only {
        filter.push_str(":force_divisible_by=2");
    }
    filter
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> ResizeSpec {
        ResizeSpec::default()
    }

    #[test]
    fn default_is_1080p_no_upscale() {
        assert_eq!(
            spec(),
            ResizeSpec {
                width: 1920,
                height: 1080,
                allow_upscale: false
            }
        );
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
