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
        "out_time_us" | "out_time_ms" => value
            .trim()
            .parse::<u64>()
            .ok()
            .map(ProgressEvent::OutTimeUs),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_out_time_us() {
        assert_eq!(
            parse_progress_line("out_time_us=1500000"),
            Some(ProgressEvent::OutTimeUs(1_500_000))
        );
    }

    #[test]
    fn out_time_ms_is_microseconds_too() {
        // ffmpeg's historical naming bug: out_time_ms is also microseconds.
        assert_eq!(
            parse_progress_line("out_time_ms=250000"),
            Some(ProgressEvent::OutTimeUs(250_000))
        );
    }

    #[test]
    fn parses_end_marker() {
        assert_eq!(
            parse_progress_line("progress=end"),
            Some(ProgressEvent::End)
        );
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
