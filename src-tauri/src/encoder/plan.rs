use std::path::{Path, PathBuf};

use crate::settings::Preset;

use super::probe::ProbeInfo;

#[derive(Clone, Debug)]
pub struct EncodePlan {
    pub video_kbit: u32,
    pub audio_kbit: u32,
    pub vf: Option<String>,
    pub output: PathBuf,
}

pub fn build_plan(info: &ProbeInfo, preset: &Preset, input: &Path) -> Result<EncodePlan, String> {
    if info.duration_secs <= 0.0 {
        return Err("Video has no measurable duration".to_string());
    }

    let target_bytes = preset.target_mb * 1_000_000.0;
    let budget_kbit = target_bytes * 8.0 / 1000.0 * 0.95; // 5% container margin
    let audio_kbit: u32 = if preset.strip_audio || !info.has_audio {
        0
    } else {
        96
    };
    // Saturates to 0 when the budget can't even cover audio.
    let video_kbit = (budget_kbit / info.duration_secs - audio_kbit as f64) as u32;
    if video_kbit < 100 {
        return Err(
            "Target size too small for this video's duration — lower FPS, scale down, or pick a bigger target"
                .to_string(),
        );
    }

    let mut filters: Vec<String> = Vec::new();
    if preset.max_width.is_some_and(|w| info.width > w) {
        let w = preset.max_width.unwrap();
        filters.push(format!("scale='min(iw,{w})':-2"));
    } else if preset.scale_percent.is_some_and(|p| p != 100) {
        let p = preset.scale_percent.unwrap();
        filters.push(format!("scale=trunc(iw*{p}/100/2)*2:-2"));
    }
    if preset.max_fps.is_some_and(|f| info.fps > f as f64) {
        filters.push(format!("fps={}", preset.max_fps.unwrap()));
    }
    let vf = if filters.is_empty() {
        None
    } else {
        Some(filters.join(","))
    };

    Ok(EncodePlan {
        video_kbit,
        audio_kbit,
        vf,
        output: unique_output(input),
    })
}

fn unique_output(input: &Path) -> PathBuf {
    let dir = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "video".to_string());
    let candidate = dir.join(format!("{stem} (tamped).mp4"));
    if !candidate.exists() {
        return candidate;
    }
    for n in 2u32.. {
        let candidate = dir.join(format!("{stem} (tamped {n}).mp4"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("ran out of output name candidates")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(target_mb: f64) -> Preset {
        Preset {
            id: "test".to_string(),
            name: "Test".to_string(),
            target_mb,
            max_fps: None,
            max_width: None,
            scale_percent: None,
            strip_audio: false,
        }
    }

    fn info() -> ProbeInfo {
        ProbeInfo {
            duration_secs: 60.0,
            width: 1920,
            height: 1080,
            fps: 60.0,
            has_audio: false,
        }
    }

    // Use a nonexistent dir so the collision probe never finds anything.
    const INPUT: &str = "/nonexistent-tamp-test/clip.mov";

    #[test]
    fn ten_mb_sixty_seconds_no_audio() {
        // budget = 10MB * 8 / 1000 * 0.95 = 76000 kbit; / 60s = 1266.66 kbit/s
        let plan = build_plan(&info(), &preset(10.0), Path::new(INPUT)).unwrap();
        assert_eq!(plan.video_kbit, 1266);
        assert_eq!(plan.audio_kbit, 0);
        assert_eq!(plan.vf, None);
        assert_eq!(
            plan.output,
            PathBuf::from("/nonexistent-tamp-test/clip (tamped).mp4")
        );
    }

    #[test]
    fn audio_budget_subtracted_when_present() {
        let info = ProbeInfo {
            has_audio: true,
            ..info()
        };
        let plan = build_plan(&info, &preset(10.0), Path::new(INPUT)).unwrap();
        assert_eq!(plan.audio_kbit, 96);
        assert_eq!(plan.video_kbit, 1170); // 1266.66 - 96
    }

    #[test]
    fn strip_audio_zeroes_audio_budget() {
        let info = ProbeInfo {
            has_audio: true,
            ..info()
        };
        let mut p = preset(10.0);
        p.strip_audio = true;
        let plan = build_plan(&info, &p, Path::new(INPUT)).unwrap();
        assert_eq!(plan.audio_kbit, 0);
        assert_eq!(plan.video_kbit, 1266);
    }

    #[test]
    fn too_small_target_errors() {
        let info = ProbeInfo {
            duration_secs: 600.0,
            ..info()
        };
        let err = build_plan(&info, &preset(0.1), Path::new(INPUT)).unwrap_err();
        assert!(err.contains("Target size too small"), "{err}");
    }

    #[test]
    fn budget_below_audio_bitrate_errors_not_panics() {
        let info = ProbeInfo {
            duration_secs: 3600.0,
            has_audio: true,
            ..info()
        };
        // budget/duration < 96 => video budget would be negative (saturates to 0)
        assert!(build_plan(&info, &preset(0.05), Path::new(INPUT)).is_err());
    }

    #[test]
    fn max_width_filter_only_when_wider() {
        let mut p = preset(10.0);
        p.max_width = Some(1280);
        let plan = build_plan(&info(), &p, Path::new(INPUT)).unwrap();
        assert_eq!(plan.vf.as_deref(), Some("scale='min(iw,1280)':-2"));

        let narrow = ProbeInfo {
            width: 1280,
            ..info()
        };
        let plan = build_plan(&narrow, &p, Path::new(INPUT)).unwrap();
        assert_eq!(plan.vf, None);
    }

    #[test]
    fn scale_percent_filter() {
        let mut p = preset(10.0);
        p.scale_percent = Some(50);
        let plan = build_plan(&info(), &p, Path::new(INPUT)).unwrap();
        assert_eq!(plan.vf.as_deref(), Some("scale=trunc(iw*50/100/2)*2:-2"));
    }

    #[test]
    fn scale_percent_100_is_noop() {
        let mut p = preset(10.0);
        p.scale_percent = Some(100);
        let plan = build_plan(&info(), &p, Path::new(INPUT)).unwrap();
        assert_eq!(plan.vf, None);
    }

    #[test]
    fn scale_percent_applies_when_max_width_not_exceeded() {
        let mut p = preset(10.0);
        p.max_width = Some(4000); // wider than the video — no max_width scaling
        p.scale_percent = Some(50);
        let plan = build_plan(&info(), &p, Path::new(INPUT)).unwrap();
        assert_eq!(plan.vf.as_deref(), Some("scale=trunc(iw*50/100/2)*2:-2"));
    }

    #[test]
    fn fps_filter_only_when_above_cap() {
        let mut p = preset(10.0);
        p.max_fps = Some(30);
        let plan = build_plan(&info(), &p, Path::new(INPUT)).unwrap();
        assert_eq!(plan.vf.as_deref(), Some("fps=30"));

        let slow = ProbeInfo {
            fps: 24.0,
            ..info()
        };
        let plan = build_plan(&slow, &p, Path::new(INPUT)).unwrap();
        assert_eq!(plan.vf, None);
    }

    #[test]
    fn filters_combine_in_order() {
        let mut p = preset(10.0);
        p.max_width = Some(1280);
        p.max_fps = Some(30);
        let plan = build_plan(&info(), &p, Path::new(INPUT)).unwrap();
        assert_eq!(plan.vf.as_deref(), Some("scale='min(iw,1280)':-2,fps=30"));
    }

    #[test]
    fn output_name_avoids_collisions() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("rec.mov");
        std::fs::write(&input, b"x").unwrap();

        let plan = build_plan(&info(), &preset(10.0), &input).unwrap();
        assert_eq!(plan.output, dir.path().join("rec (tamped).mp4"));

        std::fs::write(dir.path().join("rec (tamped).mp4"), b"x").unwrap();
        let plan = build_plan(&info(), &preset(10.0), &input).unwrap();
        assert_eq!(plan.output, dir.path().join("rec (tamped 2).mp4"));

        std::fs::write(dir.path().join("rec (tamped 2).mp4"), b"x").unwrap();
        let plan = build_plan(&info(), &preset(10.0), &input).unwrap();
        assert_eq!(plan.output, dir.path().join("rec (tamped 3).mp4"));
    }
}
