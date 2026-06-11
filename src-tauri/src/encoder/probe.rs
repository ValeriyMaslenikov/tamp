use std::path::Path;

#[derive(Clone, Debug)]
pub struct ProbeInfo {
    pub duration_secs: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub has_audio: bool,
}

pub async fn probe(path: &Path) -> Result<ProbeInfo, String> {
    let output = tokio::process::Command::new(super::bin::ffprobe_path())
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-show_entries",
            "stream=codec_type,width,height,avg_frame_rate",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .await
        .map_err(|e| format!("failed to run ffprobe: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "ffprobe failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("invalid ffprobe output: {e}"))?;

    let duration_secs = json["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or_else(|| "ffprobe reported no duration".to_string())?;

    let empty = Vec::new();
    let streams = json["streams"].as_array().unwrap_or(&empty);
    let video = streams
        .iter()
        .find(|s| s["codec_type"] == "video")
        .ok_or_else(|| "no video stream found".to_string())?;

    Ok(ProbeInfo {
        duration_secs,
        width: video["width"].as_u64().unwrap_or(0) as u32,
        height: video["height"].as_u64().unwrap_or(0) as u32,
        fps: parse_frame_rate(video["avg_frame_rate"].as_str().unwrap_or("")),
        has_audio: streams.iter().any(|s| s["codec_type"] == "audio"),
    })
}

fn parse_frame_rate(rate: &str) -> f64 {
    match rate.split_once('/') {
        Some((num, den)) => {
            let num: f64 = num.parse().unwrap_or(0.0);
            let den: f64 = den.parse().unwrap_or(0.0);
            if den > 0.0 {
                num / den
            } else {
                0.0
            }
        }
        None => rate.parse().unwrap_or(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_frame_rate;

    #[test]
    fn parses_rational_frame_rates() {
        assert!((parse_frame_rate("30000/1001") - 29.97).abs() < 0.01);
        assert_eq!(parse_frame_rate("30/1"), 30.0);
        assert_eq!(parse_frame_rate("0/0"), 0.0);
        assert_eq!(parse_frame_rate("60"), 60.0);
        assert_eq!(parse_frame_rate(""), 0.0);
    }
}
