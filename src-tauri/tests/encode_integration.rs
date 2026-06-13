//! End-to-end exercise of probe -> plan -> two-pass encode using the bundled
//! ffmpeg binaries (src-tauri/binaries). No AppHandle needed: it drives the
//! same `run_passes` pipeline the worker uses.

use std::path::{Path, PathBuf};
use std::process::Command;

use tamp_lib::encoder::{
    bin,
    plan::{build_plan, expected_output, expected_part_outputs, plan_split, preset_hash},
    probe::probe,
    run_gif, run_hardware_pass, run_passes, run_video_convergence, ChildSlot, OutputFormat, Preset,
    SplitConfig, SplitMode, StaticSplitBy,
};

fn make_clip(dir: &Path, duration_secs: u32) -> PathBuf {
    let input = dir.join("clip.mp4");
    let status = Command::new(bin::ffmpeg_path())
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc2=size=640x360:rate=30:duration={duration_secs}"),
            "-f",
            "lavfi",
            "-i",
            &format!("sine=frequency=440:sample_rate=44100:duration={duration_secs}"),
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            // 4:4:4 like real screen captures — the pipeline must force the
            // output back to 4:2:0 or QuickTime/Discord can't play it.
            "-pix_fmt",
            "yuv444p",
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            "-shortest",
        ])
        .arg(&input)
        .status()
        .expect("failed to run bundled ffmpeg — did scripts/fetch-ffmpeg.sh run?");
    assert!(status.success(), "test clip generation failed");
    input
}

fn make_test_clip(dir: &Path) -> PathBuf {
    make_clip(dir, 3)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[tokio::test]
async fn probe_falls_back_to_packet_timestamps_for_durationless_webm() {
    // MediaRecorder-style WebM: mux to a pipe so the muxer can't seek back to
    // write a container duration — probe must fall back to packet timestamps.
    let out = Command::new(bin::ffmpeg_path())
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=30:duration=2",
            "-c:v",
            "libvpx",
            "-deadline",
            "realtime",
            "-f",
            "webm",
            "-",
        ])
        .output()
        .expect("failed to run bundled ffmpeg — did scripts/fetch-ffmpeg.sh run?");
    assert!(out.status.success(), "webm generation failed");

    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("rec.webm");
    std::fs::write(&input, &out.stdout).unwrap();

    let info = probe(&input)
        .await
        .expect("probe must handle WebMs without container duration");
    assert!(
        (1.5..2.5).contains(&info.duration_secs),
        "unexpected duration {}",
        info.duration_secs
    );
    assert_eq!(info.width, 320);
    assert_eq!(info.height, 180);
    assert!(!info.has_audio);
}

#[tokio::test]
async fn two_pass_encode_hits_target_size() {
    let dir = tempfile::tempdir().unwrap();
    let input = make_test_clip(dir.path());

    let info = probe(&input).await.expect("probe failed");
    assert!(
        (2.5..4.0).contains(&info.duration_secs),
        "unexpected duration {}",
        info.duration_secs
    );
    assert_eq!(info.width, 640);
    assert_eq!(info.height, 360);
    assert!(
        (29.0..31.0).contains(&info.fps),
        "unexpected fps {}",
        info.fps
    );
    assert!(info.has_audio);

    let preset = Preset {
        id: "test-1mb".to_string(),
        name: "Test (1MB)".to_string(),
        target_mb: 1.0,
        max_fps: None,
        max_width: None,
        scale_percent: None,
        strip_audio: false,
        format: OutputFormat::Mp4,
        split: SplitConfig::default(),
    };
    let plan = build_plan(&info, &preset, &input).expect("build_plan failed");
    assert_eq!(plan.audio_kbit, 96);
    assert_eq!(
        plan.vf.as_deref(),
        Some("scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p")
    );
    assert!(plan.video_kbit >= 100);
    assert_eq!(
        plan.output,
        expected_output(
            &input,
            &preset.name,
            &preset_hash(&preset),
            OutputFormat::Mp4
        )
    );
    assert_eq!(
        plan.output.file_name().unwrap().to_string_lossy(),
        format!("clip (tamped Test 1MB {}).mp4", preset_hash(&preset))
    );

    let passlog = tempfile::tempdir().unwrap();
    let slot = ChildSlot::default();
    let mut seen: Vec<(u8, f64)> = Vec::new();
    run_passes(
        &plan,
        &info,
        &input,
        passlog.path(),
        &slot,
        &|| false,
        &mut |pass, overall| seen.push((pass, overall)),
    )
    .await
    .expect("run_passes failed");

    assert!(
        seen.iter().any(|(pass, _)| *pass == 1),
        "no pass 1 progress"
    );
    assert!(
        seen.iter().any(|(pass, _)| *pass == 2),
        "no pass 2 progress"
    );
    assert_eq!(seen.last().map(|(_, overall)| *overall), Some(1.0));
    // Pass 1 progress must stay in 0..0.5, pass 2 in 0.5..1.0.
    assert!(seen.iter().all(|(pass, overall)| if *pass == 1 {
        *overall <= 0.5
    } else {
        *overall >= 0.5
    }));

    let bytes = std::fs::read(&plan.output).expect("output file missing");
    let target_bytes = preset.target_mb * 1_000_000.0;
    assert!(
        (bytes.len() as f64) <= target_bytes,
        "output {} bytes exceeds {} byte target",
        bytes.len(),
        target_bytes
    );
    assert!(
        bytes.len() > 10_000,
        "output suspiciously small: {} bytes",
        bytes.len()
    );

    // +faststart relocates the moov atom ahead of mdat.
    let moov = find(&bytes, b"moov").expect("no moov atom in output");
    let mdat = find(&bytes, b"mdat").expect("no mdat atom in output");
    assert!(
        moov < mdat,
        "moov ({moov}) should precede mdat ({mdat}) with +faststart"
    );

    // The 4:4:4 source must come out as 4:2:0 for player compatibility.
    let out = Command::new(bin::ffprobe_path())
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=pix_fmt",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(&plan.output)
        .output()
        .expect("failed to run bundled ffprobe");
    assert!(out.status.success(), "ffprobe on output failed");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "yuv420p",
        "output must be 4:2:0"
    );
}

#[tokio::test]
async fn hardware_single_pass_spans_full_progress_range() {
    // The probed candidate list (platform preference order ∩ what the
    // bundled ffmpeg ships). Empty means hardware never runs here.
    let candidates = tamp_lib::encoder::hw::available_candidates().await;
    if candidates.is_empty() {
        eprintln!("skipping hardware encode test: no hardware candidates available");
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let input = make_test_clip(dir.path());
    let info = probe(&input).await.expect("probe failed");

    let preset = Preset {
        id: "test-1mb-hw".to_string(),
        name: "Test HW (1MB)".to_string(),
        target_mb: 1.0,
        max_fps: None,
        max_width: None,
        scale_percent: None,
        strip_audio: false,
        format: OutputFormat::Mp4,
        split: SplitConfig::default(),
    };
    let plan = build_plan(&info, &preset, &input).expect("build_plan failed");

    let slot = ChildSlot::default();
    let mut seen: Vec<(u8, f64)> = Vec::new();
    // Like the worker: a candidate may be compiled in but unusable on this
    // machine (no GPU session in CI sandboxes/VMs) — fall through to the
    // next, and skip the test only when none of them can encode.
    let mut encoded = false;
    for cand in candidates {
        seen.clear();
        match run_hardware_pass(
            cand,
            &plan,
            &info,
            &input,
            &slot,
            &|| false,
            &mut |pass, overall| seen.push((pass, overall)),
        )
        .await
        {
            Ok(()) => {
                encoded = true;
                break;
            }
            Err(e) => eprintln!("{} unavailable here: {e}", cand.name),
        }
    }
    if !encoded {
        eprintln!("skipping hardware encode test: no candidate could encode on this machine");
        return;
    }

    // The single pass must map onto the FULL 0..1 range, reported as pass 1.
    assert!(!seen.is_empty(), "no progress reported");
    assert!(seen.iter().all(|(pass, _)| *pass == 1), "{seen:?}");
    assert!(
        seen.iter().all(|(_, o)| (0.0..=1.0).contains(o)),
        "{seen:?}"
    );
    assert_eq!(seen.first().map(|(_, o)| *o), Some(0.0));
    assert_eq!(seen.last().map(|(_, o)| *o), Some(1.0));
    assert!(
        seen.iter().any(|(_, o)| *o > 0.5),
        "progress never crossed the two-pass halfway mark: {seen:?}"
    );

    let bytes = std::fs::read(&plan.output).expect("output file missing");
    assert!(
        bytes.len() > 10_000,
        "output suspiciously small: {} bytes",
        bytes.len()
    );

    // +faststart relocates the moov atom ahead of mdat.
    let moov = find(&bytes, b"moov").expect("no moov atom in output");
    let mdat = find(&bytes, b"mdat").expect("no mdat atom in output");
    assert!(
        moov < mdat,
        "moov ({moov}) should precede mdat ({mdat}) with +faststart"
    );

    // The 4:4:4 source must come out as 4:2:0 with audio preserved.
    let out = Command::new(bin::ffprobe_path())
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,codec_name,pix_fmt",
            "-of",
            "json",
        ])
        .arg(&plan.output)
        .output()
        .expect("failed to run bundled ffprobe");
    assert!(out.status.success(), "ffprobe on output failed");
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let streams = json["streams"].as_array().unwrap();
    let video = streams
        .iter()
        .find(|s| s["codec_type"] == "video")
        .expect("no video stream in output");
    assert_eq!(video["codec_name"], "h264");
    assert_eq!(video["pix_fmt"], "yuv420p", "output must be 4:2:0");
    assert!(
        streams.iter().any(|s| s["codec_type"] == "audio"),
        "audio stream must be preserved"
    );
}

#[tokio::test]
async fn webm_two_pass_vp9_hits_target_size() {
    let dir = tempfile::tempdir().unwrap();
    let input = make_test_clip(dir.path());
    let info = probe(&input).await.expect("probe failed");

    let preset = Preset {
        id: "test-1mb-webm".to_string(),
        name: "Test WebM (1MB)".to_string(),
        target_mb: 1.0,
        max_fps: None,
        max_width: None,
        scale_percent: None,
        strip_audio: false,
        format: OutputFormat::Webm,
        split: SplitConfig::default(),
    };
    let plan = build_plan(&info, &preset, &input).expect("build_plan failed");
    assert_eq!(plan.audio_kbit, 64); // opus budget, not the 96k aac one
    assert!(plan.video_kbit >= 100);
    assert_eq!(
        plan.output,
        expected_output(
            &input,
            &preset.name,
            &preset_hash(&preset),
            OutputFormat::Webm
        )
    );

    let passlog = tempfile::tempdir().unwrap();
    let slot = ChildSlot::default();
    let mut seen: Vec<(u8, f64)> = Vec::new();
    run_passes(
        &plan,
        &info,
        &input,
        passlog.path(),
        &slot,
        &|| false,
        &mut |pass, overall| seen.push((pass, overall)),
    )
    .await
    .expect("run_passes failed");

    // Same two-pass progress mapping as mp4: pass 1 in 0..0.5, pass 2 in 0.5..1.0.
    assert!(
        seen.iter().any(|(pass, _)| *pass == 1),
        "no pass 1 progress"
    );
    assert!(
        seen.iter().any(|(pass, _)| *pass == 2),
        "no pass 2 progress"
    );
    assert_eq!(seen.last().map(|(_, overall)| *overall), Some(1.0));
    assert!(seen.iter().all(|(pass, overall)| if *pass == 1 {
        *overall <= 0.5
    } else {
        *overall >= 0.5
    }));

    let bytes = std::fs::metadata(&plan.output).expect("output file missing");
    let target_bytes = preset.target_mb * 1_000_000.0;
    assert!(
        (bytes.len() as f64) <= target_bytes,
        "output {} bytes exceeds {} byte target",
        bytes.len(),
        target_bytes
    );
    assert!(
        bytes.len() > 10_000,
        "output suspiciously small: {} bytes",
        bytes.len()
    );

    // The video must really be VP9 with the audio transcoded to Opus.
    let out = Command::new(bin::ffprobe_path())
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,codec_name",
            "-of",
            "json",
        ])
        .arg(&plan.output)
        .output()
        .expect("failed to run bundled ffprobe");
    assert!(out.status.success(), "ffprobe on output failed");
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let streams = json["streams"].as_array().unwrap();
    let video = streams
        .iter()
        .find(|s| s["codec_type"] == "video")
        .expect("no video stream in output");
    assert_eq!(video["codec_name"], "vp9");
    let audio = streams
        .iter()
        .find(|s| s["codec_type"] == "audio")
        .expect("audio stream must be preserved");
    assert_eq!(audio["codec_name"], "opus");
}

#[tokio::test]
async fn gif_palette_encode_stays_under_generous_target() {
    let dir = tempfile::tempdir().unwrap();
    let input = make_test_clip(dir.path());
    let info = probe(&input).await.expect("probe failed");

    let preset = Preset {
        id: "test-2mb-gif".to_string(),
        name: "Test GIF (2MB)".to_string(),
        target_mb: 2.0,
        max_fps: None,
        max_width: None,
        scale_percent: None,
        strip_audio: false,
        format: OutputFormat::Gif,
        split: SplitConfig::default(),
    };
    let plan = build_plan(&info, &preset, &input).expect("build_plan failed");
    assert_eq!(plan.video_kbit, 0); // bitrate math doesn't apply to GIF
    assert_eq!(plan.audio_kbit, 0); // GIF never carries audio
    assert_eq!(
        plan.output,
        expected_output(
            &input,
            &preset.name,
            &preset_hash(&preset),
            OutputFormat::Gif
        )
    );

    let target_bytes = preset.target_mb * 1_000_000.0;
    let slot = ChildSlot::default();
    let mut seen: Vec<(u8, f64)> = Vec::new();
    let actual = run_gif(
        &plan,
        &info,
        &input,
        target_bytes,
        &slot,
        &|| false,
        &mut |pass, overall| seen.push((pass, overall)),
    )
    .await
    .expect("run_gif failed");

    // Every attempt maps onto the FULL 0..1 range, reported as pass 1.
    assert!(!seen.is_empty(), "no progress reported");
    assert!(seen.iter().all(|(pass, _)| *pass == 1), "{seen:?}");
    assert!(
        seen.iter().all(|(_, o)| (0.0..=1.0).contains(o)),
        "{seen:?}"
    );
    assert_eq!(seen.last().map(|(_, o)| *o), Some(1.0));

    let bytes = std::fs::read(&plan.output).expect("output file missing");
    assert!(
        bytes.len() > 1_000,
        "output suspiciously small: {} bytes",
        bytes.len()
    );
    assert!(
        bytes.starts_with(b"GIF89a") || bytes.starts_with(b"GIF87a"),
        "output is not a GIF"
    );
    // The retry loop must land a 3s 480px clip comfortably under this
    // generous target; when it can't fit at all it now FAILS (deleting the
    // file) instead of keeping an oversized gif.
    assert!(
        (bytes.len() as f64) <= target_bytes,
        "gif {} bytes exceeds {} byte target",
        bytes.len(),
        target_bytes
    );
    assert_eq!(
        actual,
        bytes.len() as u64,
        "run_gif must report the on-disk size"
    );
}

/// High-entropy grayscale noise — nearly incompressible, the hardest case
/// for rate control and the deterministic way to stress the convergence loop.
fn make_noise_clip(dir: &Path) -> PathBuf {
    let input = dir.join("noise.mp4");
    let status = Command::new(bin::ffmpeg_path())
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "nullsrc=size=640x360:rate=30,geq=random(1)*255:128:128",
            "-t",
            "2",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&input)
        .status()
        .expect("failed to run bundled ffmpeg — did scripts/fetch-ffmpeg.sh run?");
    assert!(status.success(), "noise clip generation failed");
    input
}

#[tokio::test]
async fn convergence_loop_enforces_hard_cap_on_high_entropy_source() {
    let dir = tempfile::tempdir().unwrap();
    let input = make_noise_clip(dir.path());
    let info = probe(&input).await.expect("probe failed");

    // Tight but achievable: ~1500 kbit/s of pure noise is well within what
    // two-pass x264 can rate-control, but leaves little slack — exactly the
    // regime where single attempts historically overshot.
    let preset = Preset {
        id: "test-tight".to_string(),
        name: "Tight (0.4MB)".to_string(),
        target_mb: 0.4,
        max_fps: None,
        max_width: None,
        scale_percent: None,
        strip_audio: false,
        format: OutputFormat::Mp4,
        split: SplitConfig::default(),
    };
    let mut plan = build_plan(&info, &preset, &input).expect("build_plan failed");
    let target_bytes = preset.target_mb * 1_000_000.0;

    let passlog = tempfile::tempdir().unwrap();
    let slot = ChildSlot::default();
    let actual = run_video_convergence(
        &mut plan,
        &info,
        &input,
        passlog.path(),
        target_bytes,
        false, // software start: the deterministic two-pass path
        &slot,
        &|| false,
        &mut |_, _| {},
    )
    .await
    .expect("convergence loop failed");

    let on_disk = std::fs::metadata(&plan.output)
        .expect("output file missing")
        .len();
    assert_eq!(actual, on_disk, "reported size must match the file");
    // The HARD guarantee: a delivered output is never over target, not even
    // by one byte.
    assert!(
        (on_disk as f64) <= target_bytes,
        "output {on_disk} bytes exceeds {target_bytes} byte target"
    );
    assert!(
        on_disk > 10_000,
        "output suspiciously small: {on_disk} bytes"
    );
}

#[tokio::test]
async fn impossible_gif_target_fails_and_removes_the_output() {
    let dir = tempfile::tempdir().unwrap();
    let input = make_noise_clip(dir.path());
    let info = probe(&input).await.expect("probe failed");

    // 50 KB of noise GIF is impossible even at the 160px/8fps floors: the
    // engine must exhaust its width+fps retries, DELETE the oversized file
    // and fail with the actionable error — never keep an over-target gif.
    let preset = Preset {
        id: "test-tiny-gif".to_string(),
        name: "Tiny GIF".to_string(),
        target_mb: 0.05,
        max_fps: None,
        max_width: None,
        scale_percent: None,
        strip_audio: false,
        format: OutputFormat::Gif,
        split: SplitConfig::default(),
    };
    let plan = build_plan(&info, &preset, &input).expect("build_plan failed");
    let target_bytes = preset.target_mb * 1_000_000.0;

    let slot = ChildSlot::default();
    let err = run_gif(
        &plan,
        &info,
        &input,
        target_bytes,
        &slot,
        &|| false,
        &mut |_, _| {},
    )
    .await
    .expect_err("a 50 KB noise gif must fail, not deliver an oversized file");
    assert!(err.contains("Couldn't fit under 0.05 MB"), "{err}");
    assert!(
        err.contains("lower the resolution/FPS in the preset or pick a bigger target"),
        "{err}"
    );
    assert!(
        !plan.output.exists(),
        "the oversized gif must be deleted, not kept"
    );
    assert!(
        !tamp_lib::encoder::plan::part_path(&plan.output).exists(),
        "the in-flight part file must be removed on failure"
    );
}

/// Duration of a finished file per the bundled ffprobe, for verifying that
/// split parts really carry their slice of the input.
fn probed_duration(path: &Path) -> f64 {
    let out = Command::new(bin::ffprobe_path())
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()
        .expect("failed to run bundled ffprobe");
    assert!(out.status.success(), "ffprobe on part failed");
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .expect("unparseable duration")
}

#[tokio::test]
async fn static_split_encodes_three_parts_each_under_the_full_target() {
    // A ~25s clip split statically into 3 parts: every part runs through the
    // exact per-part pipeline the worker uses (build_plan on a part-length
    // ProbeInfo, -ss/-t trim, convergence + promote) and gets the preset's
    // FULL byte target to itself.
    let dir = tempfile::tempdir().unwrap();
    let input = make_clip(dir.path(), 25);
    let info = probe(&input).await.expect("probe failed");
    assert!(
        (24.0..26.5).contains(&info.duration_secs),
        "unexpected duration {}",
        info.duration_secs
    );

    let preset = Preset {
        id: "test-split".to_string(),
        name: "Test Split (1MB x3)".to_string(),
        target_mb: 1.0,
        max_fps: None,
        max_width: None,
        scale_percent: None,
        strip_audio: false,
        format: OutputFormat::Mp4,
        split: SplitConfig {
            mode: SplitMode::Static,
            by: StaticSplitBy::Parts,
            parts: 3,
            seconds: 30,
        },
    };
    let split = plan_split(&info, &preset);
    assert_eq!(split.count, 3);
    assert!((split.part_secs - info.duration_secs / 3.0).abs() < 1e-9);

    let hash = preset_hash(&preset);
    let parts = expected_part_outputs(&input, &preset.name, &hash, OutputFormat::Mp4, split.count);
    assert_eq!(parts.len(), 3);
    // Parts live in a single named folder, short-numbered inside it.
    let folder = parts[0].parent().unwrap();
    assert_eq!(
        folder.file_name().unwrap().to_string_lossy(),
        format!("clip (tamped Test Split 1MB x3 {hash})"),
        "the folder carries the sanitized preset name and hash"
    );
    std::fs::create_dir_all(folder).unwrap();
    for (idx, part) in parts.iter().enumerate() {
        assert_eq!(
            part.file_name().unwrap().to_string_lossy(),
            format!("clip {}.mp4", idx + 1),
            "part files are short-numbered inside the folder"
        );
    }

    let target_bytes = preset.target_mb * 1_000_000.0;
    let mut output_bytes: u64 = 0; // the worker reports the SUM of all parts
    for (idx, output) in parts.iter().enumerate() {
        let i = idx as u32 + 1;
        let start = split.part_secs * f64::from(i - 1);
        let len = if i == split.count {
            info.duration_secs - split.part_secs * f64::from(split.count - 1)
        } else {
            split.part_secs
        };
        let mut part_info = info.clone();
        part_info.duration_secs = len;
        let mut plan = build_plan(&part_info, &preset, &input).expect("build_plan failed");
        plan.output = output.clone();
        plan.trim = Some((start, len));

        let passlog = tempfile::tempdir().unwrap();
        let slot = ChildSlot::default();
        let actual = run_video_convergence(
            &mut plan,
            &part_info,
            &input,
            passlog.path(),
            target_bytes,
            false, // software: the deterministic two-pass path
            &slot,
            &|| false,
            &mut |_, _| {},
        )
        .await
        .unwrap_or_else(|e| panic!("part {i} encode failed: {e}"));
        output_bytes += actual;
    }

    let mut on_disk_sum: u64 = 0;
    for (idx, part) in parts.iter().enumerate() {
        let len = std::fs::metadata(part)
            .unwrap_or_else(|_| panic!("part {} missing at {}", idx + 1, part.display()))
            .len();
        assert!(
            (len as f64) <= target_bytes,
            "part {} is {len} bytes, over the {target_bytes} byte target",
            idx + 1
        );
        assert!(
            len > 10_000,
            "part {} suspiciously small: {len} bytes",
            idx + 1
        );
        on_disk_sum += len;
        // Each part must really carry ~a third of the clip.
        let duration = probed_duration(part);
        assert!(
            (duration - split.part_secs).abs() < 1.0,
            "part {} runs {duration}s, expected ~{}s",
            idx + 1,
            split.part_secs
        );
        // No in-flight temp may survive a successful promote.
        assert!(!tamp_lib::encoder::plan::part_path(part).exists());
    }
    assert_eq!(
        output_bytes, on_disk_sum,
        "the reported total must be the sum of all part sizes"
    );
}
