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
    let mut video_kbit = (budget_kbit / info.duration_secs - audio_kbit as f64) as u32;
    if video_kbit < 100 {
        return Err(
            "Target size too small for this video's duration — lower FPS, scale down, or pick a bigger target"
                .to_string(),
        );
    }
    // Never exceed the source's own effective bitrate — a small input with a
    // roomy target would otherwise come out BIGGER than the original.
    if let Ok(meta) = std::fs::metadata(input) {
        let source_total = meta.len() as f64 * 8.0 / 1000.0 / info.duration_secs;
        let cap = (source_total - audio_kbit as f64).max(100.0) as u32;
        video_kbit = video_kbit.min(cap);
    }

    let mut filters: Vec<String> = Vec::new();
    if preset.max_width.is_some_and(|w| info.width > w) {
        let w = preset.max_width.unwrap();
        // trunc-to-even so an odd max_width (or odd source) can't produce a
        // width libx264 rejects in 4:2:0.
        filters.push(format!("scale='trunc(min(iw,{w})/2)*2':-2"));
    } else if preset.scale_percent.is_some_and(|p| p != 100) {
        let p = preset.scale_percent.unwrap();
        filters.push(format!("scale=trunc(iw*{p}/100/2)*2:-2"));
    } else {
        // 4:2:2/4:4:4 sources can legally have odd dimensions, which libx264
        // rejects once converted to 4:2:0.
        filters.push("scale=trunc(iw/2)*2:trunc(ih/2)*2".to_string());
    }
    if preset.max_fps.is_some_and(|f| info.fps > f as f64) {
        filters.push(format!("fps={}", preset.max_fps.unwrap()));
    }
    // Always force 8-bit 4:2:0 output: screen captures are often 4:4:4 or
    // 10-bit, which would otherwise yield a High profile QuickTime/Discord
    // can't play. Must stay the last filter in the chain.
    filters.push("format=yuv420p".to_string());
    let vf = Some(filters.join(","));

    Ok(EncodePlan {
        video_kbit,
        audio_kbit,
        vf,
        output: unique_output(input, &preset_hash(preset)),
    })
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv1a(hash: u64, bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(hash, |h, &b| (h ^ u64::from(b)).wrapping_mul(FNV_PRIME))
}

/// Stable 4-hex-char fingerprint of a preset's encode-affecting configuration.
///
/// Embedded in output names ("clip (tamped 823f).mp4") so a re-run with the
/// same configuration can recognise and reuse an existing output. The hash
/// must stay identical across releases and runs, hence an inline FNV-1a 64
/// (std's `DefaultHasher` makes no stability guarantee) over the fields in a
/// fixed order: `target_mb` bits (u64 LE), then `max_fps` / `max_width` /
/// `scale_percent` (u32 LE, `u32::MAX` for `None`), then `strip_audio` (0/1).
/// Cosmetic fields (id, name) are deliberately excluded.
pub fn preset_hash(p: &crate::settings::Preset) -> String {
    let mut h = fnv1a(FNV_OFFSET, &p.target_mb.to_bits().to_le_bytes());
    h = fnv1a(h, &p.max_fps.unwrap_or(u32::MAX).to_le_bytes());
    h = fnv1a(h, &p.max_width.unwrap_or(u32::MAX).to_le_bytes());
    h = fnv1a(h, &p.scale_percent.unwrap_or(u32::MAX).to_le_bytes());
    h = fnv1a(h, &[u8::from(p.strip_audio)]);
    format!("{:04x}", h & 0xffff)
}

fn input_stem(input: &Path) -> String {
    input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "video".to_string())
}

/// The collision-free base output path for `input` under a preset `hash`:
/// "{stem} (tamped {hash}).mp4" next to the input. The worker probes this
/// exact path before encoding — when it already exists the previous output
/// is reused instead of re-encoding.
pub fn expected_output(input: &Path, hash: &str) -> PathBuf {
    let dir = input.parent().unwrap_or_else(|| Path::new("."));
    dir.join(format!("{} (tamped {hash}).mp4", input_stem(input)))
}

fn unique_output(input: &Path, hash: &str) -> PathBuf {
    let candidate = expected_output(input, hash);
    if !candidate.exists() {
        return candidate;
    }
    let dir = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input_stem(input);
    for n in 2u32.. {
        let candidate = dir.join(format!("{stem} (tamped {hash} {n}).mp4"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("ran out of output name candidates")
}

/// Recognises tamp output stems, returning the derived original stem (the
/// stem with the whole tamp suffix removed). The scanner and the planner
/// must agree on this pattern. Accepted suffixes:
/// " (tamped)" / " (tamped 2)" (legacy, pre-hash) and
/// " (tamped 823f)" / " (tamped 823f 2)" (current, 4 lowercase hex chars).
pub fn output_original_stem(stem: &str) -> Option<&str> {
    const MARKER: &str = " (tamped";
    let inner = stem.strip_suffix(')')?;
    let idx = inner.rfind(MARKER)?;
    let original = &inner[..idx];
    let rest = &inner[idx + MARKER.len()..];
    if rest.is_empty() {
        return Some(original); // legacy "{stem} (tamped)"
    }
    let mut tokens = rest.strip_prefix(' ')?.split(' ');
    let first = tokens.next()?;
    let second = tokens.next();
    if tokens.next().is_some() {
        return None;
    }
    let is_hash = first.len() == 4
        && first
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    let is_counter = |t: &str| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit());
    let recognised = match second {
        // "{stem} (tamped 823f)" or legacy "{stem} (tamped 2)"
        None => is_hash || is_counter(first),
        // "{stem} (tamped 823f 2)"
        Some(second) => is_hash && is_counter(second),
    };
    recognised.then_some(original)
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
        assert_eq!(
            plan.vf.as_deref(),
            Some("scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p")
        );
        assert_eq!(
            plan.output,
            PathBuf::from("/nonexistent-tamp-test/clip (tamped 823f).mp4")
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
        assert_eq!(
            plan.vf.as_deref(),
            Some("scale='trunc(min(iw,1280)/2)*2':-2,format=yuv420p")
        );

        let narrow = ProbeInfo {
            width: 1280,
            ..info()
        };
        let plan = build_plan(&narrow, &p, Path::new(INPUT)).unwrap();
        assert_eq!(
            plan.vf.as_deref(),
            Some("scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p")
        );
    }

    #[test]
    fn odd_max_width_yields_even_scale_expression() {
        let mut p = preset(10.0);
        p.max_width = Some(1281);
        let plan = build_plan(&info(), &p, Path::new(INPUT)).unwrap();
        assert_eq!(
            plan.vf.as_deref(),
            Some("scale='trunc(min(iw,1281)/2)*2':-2,format=yuv420p")
        );
    }

    #[test]
    fn scale_percent_filter() {
        let mut p = preset(10.0);
        p.scale_percent = Some(50);
        let plan = build_plan(&info(), &p, Path::new(INPUT)).unwrap();
        assert_eq!(
            plan.vf.as_deref(),
            Some("scale=trunc(iw*50/100/2)*2:-2,format=yuv420p")
        );
    }

    #[test]
    fn scale_percent_100_is_noop() {
        let mut p = preset(10.0);
        p.scale_percent = Some(100);
        let plan = build_plan(&info(), &p, Path::new(INPUT)).unwrap();
        assert_eq!(
            plan.vf.as_deref(),
            Some("scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p")
        );
    }

    #[test]
    fn scale_percent_applies_when_max_width_not_exceeded() {
        let mut p = preset(10.0);
        p.max_width = Some(4000); // wider than the video — no max_width scaling
        p.scale_percent = Some(50);
        let plan = build_plan(&info(), &p, Path::new(INPUT)).unwrap();
        assert_eq!(
            plan.vf.as_deref(),
            Some("scale=trunc(iw*50/100/2)*2:-2,format=yuv420p")
        );
    }

    #[test]
    fn fps_filter_only_when_above_cap() {
        let mut p = preset(10.0);
        p.max_fps = Some(30);
        let plan = build_plan(&info(), &p, Path::new(INPUT)).unwrap();
        assert_eq!(
            plan.vf.as_deref(),
            Some("scale=trunc(iw/2)*2:trunc(ih/2)*2,fps=30,format=yuv420p")
        );

        let slow = ProbeInfo {
            fps: 24.0,
            ..info()
        };
        let plan = build_plan(&slow, &p, Path::new(INPUT)).unwrap();
        assert_eq!(
            plan.vf.as_deref(),
            Some("scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p")
        );
    }

    #[test]
    fn filters_combine_in_order() {
        let mut p = preset(10.0);
        p.max_width = Some(1280);
        p.max_fps = Some(30);
        let plan = build_plan(&info(), &p, Path::new(INPUT)).unwrap();
        assert_eq!(
            plan.vf.as_deref(),
            Some("scale='trunc(min(iw,1280)/2)*2':-2,fps=30,format=yuv420p")
        );
    }

    #[test]
    fn vf_always_present_and_ends_with_yuv420p_format() {
        let mut scaled = preset(10.0);
        scaled.max_width = Some(1280);
        scaled.max_fps = Some(30);
        let mut percent = preset(10.0);
        percent.scale_percent = Some(50);
        for p in [preset(10.0), scaled, percent] {
            let plan = build_plan(&info(), &p, Path::new(INPUT)).unwrap();
            let vf = plan.vf.expect("vf must always be present");
            assert!(vf.ends_with("format=yuv420p"), "{vf}");
        }
    }

    #[test]
    fn output_name_avoids_collisions() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("rec.mov");
        std::fs::write(&input, b"x").unwrap();
        let hash = preset_hash(&preset(10.0));

        let plan = build_plan(&info(), &preset(10.0), &input).unwrap();
        assert_eq!(
            plan.output,
            dir.path().join(format!("rec (tamped {hash}).mp4"))
        );

        std::fs::write(dir.path().join(format!("rec (tamped {hash}).mp4")), b"x").unwrap();
        let plan = build_plan(&info(), &preset(10.0), &input).unwrap();
        assert_eq!(
            plan.output,
            dir.path().join(format!("rec (tamped {hash} 2).mp4"))
        );

        std::fs::write(dir.path().join(format!("rec (tamped {hash} 2).mp4")), b"x").unwrap();
        let plan = build_plan(&info(), &preset(10.0), &input).unwrap();
        assert_eq!(
            plan.output,
            dir.path().join(format!("rec (tamped {hash} 3).mp4"))
        );
    }

    #[test]
    fn output_name_ignores_legacy_and_other_hash_collisions() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("rec.mov");
        std::fs::write(&input, b"x").unwrap();
        // Legacy outputs and outputs of other preset configs must not bump
        // the collision counter — only the exact hashed name collides.
        std::fs::write(dir.path().join("rec (tamped).mp4"), b"x").unwrap();
        std::fs::write(dir.path().join("rec (tamped 2).mp4"), b"x").unwrap();
        std::fs::write(dir.path().join("rec (tamped ffff).mp4"), b"x").unwrap();

        let hash = preset_hash(&preset(10.0));
        let plan = build_plan(&info(), &preset(10.0), &input).unwrap();
        assert_eq!(
            plan.output,
            dir.path().join(format!("rec (tamped {hash}).mp4"))
        );
    }

    #[test]
    fn video_bitrate_capped_at_source_bitrate() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("small.mov");
        // 60s file of 1.5MB -> source ~200 kbit/s, far below the 10MB budget's
        // 1266 kbit/s; the plan must not inflate the output.
        std::fs::write(&input, vec![0u8; 1_500_000]).unwrap();
        let plan = build_plan(&info(), &preset(10.0), &input).unwrap();
        assert_eq!(plan.video_kbit, 200);

        // A heavy source (40MB over 60s ≈ 5333 kbit/s) stays budget-bound.
        let input = dir.path().join("big.mov");
        let file = std::fs::File::create(&input).unwrap();
        file.set_len(40_000_000).unwrap(); // sparse, no real 40MB write
        let plan = build_plan(&info(), &preset(10.0), &input).unwrap();
        assert_eq!(plan.video_kbit, 1266);
    }

    #[test]
    fn expected_output_is_the_base_hashed_name() {
        assert_eq!(
            expected_output(Path::new(INPUT), "823f"),
            PathBuf::from("/nonexistent-tamp-test/clip (tamped 823f).mp4")
        );
    }

    // The pinned values below were computed independently (reference FNV-1a
    // implementation); they must NEVER change across releases — reuse and
    // orphan detection depend on old outputs hashing the same forever.
    #[test]
    fn preset_hash_is_stable_across_releases() {
        assert_eq!(preset_hash(&preset(10.0)), "823f");
        assert_eq!(preset_hash(&preset(1.0)), "d6e4");

        let full = Preset {
            id: "x".to_string(),
            name: "X".to_string(),
            target_mb: 8.0,
            max_fps: Some(30),
            max_width: Some(1280),
            scale_percent: Some(50),
            strip_audio: true,
        };
        assert_eq!(preset_hash(&full), "eb3d");
    }

    #[test]
    fn preset_hash_differs_per_field() {
        let base = preset(10.0);
        let target = preset(10.5);
        let mut fps = preset(10.0);
        fps.max_fps = Some(30);
        let mut width = preset(10.0);
        width.max_width = Some(1280);
        let mut scale = preset(10.0);
        scale.scale_percent = Some(50);
        let mut audio = preset(10.0);
        audio.strip_audio = true;

        let hashes: Vec<String> = [&base, &target, &fps, &width, &scale, &audio]
            .iter()
            .map(|p| preset_hash(p))
            .collect();
        for (i, a) in hashes.iter().enumerate() {
            assert_eq!(a.len(), 4, "{a}");
            assert!(a.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
            for b in hashes.iter().skip(i + 1) {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn preset_hash_ignores_cosmetic_fields() {
        let mut renamed = preset(10.0);
        renamed.id = "other-id".to_string();
        renamed.name = "Other Name".to_string();
        assert_eq!(preset_hash(&preset(10.0)), preset_hash(&renamed));
    }

    #[test]
    fn recognises_output_stems_and_derives_original() {
        // legacy, pre-hash
        assert_eq!(output_original_stem("clip (tamped)"), Some("clip"));
        assert_eq!(output_original_stem("clip (tamped 2)"), Some("clip"));
        assert_eq!(output_original_stem("clip (tamped 42)"), Some("clip"));
        // current, hashed
        assert_eq!(output_original_stem("clip (tamped 823f)"), Some("clip"));
        assert_eq!(output_original_stem("clip (tamped 823f 2)"), Some("clip"));
        assert_eq!(output_original_stem("clip (tamped 0042 12)"), Some("clip"));
        // outputs of outputs keep the inner suffix
        assert_eq!(
            output_original_stem("clip (tamped) (tamped 823f)"),
            Some("clip (tamped)")
        );
    }

    #[test]
    fn rejects_non_output_stems() {
        for stem in [
            "clip",
            "retamped",
            "clip (tamped",
            "clip (tamped )",
            "clip (tamped x)",
            "clip (tamped xyz)",
            "clip (tamped 823F)",   // hashes are lowercase
            "clip (tamped 823f5)",  // 5 chars is neither hash nor counter
            "clip (tamped abc)",    // 3-char hex is not a hash
            "clip (tamped 823f x)", // counter must be digits
            "clip (tamped 2 823f)", // counter cannot precede the hash
            "clip (tamped 823f 2 3)",
            "clip (tamped) more",
            "(tamped)",
        ] {
            assert_eq!(output_original_stem(stem), None, "{stem}");
        }
    }
}
