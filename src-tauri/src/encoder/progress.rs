/// Parses one line of `ffmpeg -progress pipe:1` output, returning the current
/// output timestamp in seconds.
///
/// ffmpeg quirk: both `out_time_us` and `out_time_ms` carry MICROSECONDS
/// (out_time_ms predates out_time_us and was never fixed). Values may be
/// "N/A" early in an encode.
pub fn parse_progress_line(line: &str) -> Option<f64> {
    let (key, value) = line.trim().split_once('=')?;
    match key.trim() {
        "out_time_us" | "out_time_ms" => {
            let micros: f64 = value.trim().parse().ok()?;
            Some(micros / 1_000_000.0)
        }
        _ => None,
    }
}

/// Maps per-pass progress to overall job progress: pass 1 covers 0..0.5,
/// pass 2 covers 0.5..1.0.
pub fn overall(pass: u8, frac: f64) -> f64 {
    let frac = frac.clamp(0.0, 1.0);
    match pass {
        1 => frac * 0.5,
        _ => 0.5 + frac * 0.5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_out_time_us_as_seconds() {
        assert_eq!(parse_progress_line("out_time_us=1500000"), Some(1.5));
        assert_eq!(parse_progress_line("out_time_us=0"), Some(0.0));
    }

    #[test]
    fn out_time_ms_is_also_microseconds() {
        // ffmpeg quirk: out_time_ms is microseconds too.
        assert_eq!(parse_progress_line("out_time_ms=2000000"), Some(2.0));
    }

    #[test]
    fn tolerates_na_and_garbage() {
        assert_eq!(parse_progress_line("out_time_us=N/A"), None);
        assert_eq!(parse_progress_line("out_time_ms=N/A"), None);
        assert_eq!(parse_progress_line("frame=42"), None);
        assert_eq!(parse_progress_line("out_time=00:00:01.500000"), None);
        assert_eq!(parse_progress_line("no equals sign here"), None);
        assert_eq!(parse_progress_line(""), None);
    }

    #[test]
    fn tolerates_surrounding_whitespace() {
        assert_eq!(parse_progress_line("  out_time_us=500000\r"), Some(0.5));
    }

    #[test]
    fn overall_maps_passes_to_halves() {
        assert_eq!(overall(1, 0.0), 0.0);
        assert_eq!(overall(1, 0.5), 0.25);
        assert_eq!(overall(1, 1.0), 0.5);
        assert_eq!(overall(2, 0.0), 0.5);
        assert_eq!(overall(2, 0.5), 0.75);
        assert_eq!(overall(2, 1.0), 1.0);
    }

    #[test]
    fn overall_clamps_fraction() {
        assert_eq!(overall(1, 1.5), 0.5);
        assert_eq!(overall(1, -0.5), 0.0);
        assert_eq!(overall(2, -1.0), 0.5);
        assert_eq!(overall(2, 2.0), 1.0);
    }
}
