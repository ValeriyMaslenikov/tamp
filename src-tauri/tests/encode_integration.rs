//! End-to-end exercise of probe -> plan -> two-pass encode using the bundled
//! ffmpeg binaries (src-tauri/binaries). No AppHandle needed: it drives the
//! same `run_passes` pipeline the worker uses.

use std::path::{Path, PathBuf};
use std::process::Command;

use tamp_lib::encoder::{
    bin,
    plan::{build_plan, preset_hash},
    probe::probe,
    run_gif, run_hardware_pass, run_passes, ChildSlot, OutputFormat, Preset,
};

fn make_test_clip(dir: &Path) -> PathBuf {
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
            "testsrc2=size=640x360:rate=30:duration=3",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=44100:duration=3",
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
        dir.path()
            .join(format!("clip (tamped {}).mp4", preset_hash(&preset)))
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
    // Skip when the bundled ffmpeg has no h264_videotoolbox encoder at all.
    let out = Command::new(bin::ffmpeg_path())
        .args(["-hide_banner", "-encoders"])
        .output()
        .expect("failed to run bundled ffmpeg — did scripts/fetch-ffmpeg.sh run?");
    if !String::from_utf8_lossy(&out.stdout).contains("h264_videotoolbox") {
        eprintln!("skipping hardware encode test: h264_videotoolbox is not in this ffmpeg build");
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
    };
    let plan = build_plan(&info, &preset, &input).expect("build_plan failed");

    let slot = ChildSlot::default();
    let mut seen: Vec<(u8, f64)> = Vec::new();
    if let Err(e) = run_hardware_pass(
        &plan,
        &info,
        &input,
        &slot,
        &|| false,
        &mut |pass, overall| seen.push((pass, overall)),
    )
    .await
    {
        // Listed but unusable (e.g. no VideoToolbox session in CI sandboxes);
        // the app falls back to libx264 in this case, so skip rather than fail.
        eprintln!("skipping hardware encode test: h264_videotoolbox unavailable here: {e}");
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
    };
    let plan = build_plan(&info, &preset, &input).expect("build_plan failed");
    assert_eq!(plan.audio_kbit, 64); // opus budget, not the 96k aac one
    assert!(plan.video_kbit >= 100);
    assert_eq!(
        plan.output,
        dir.path()
            .join(format!("clip (tamped {}).webm", preset_hash(&preset)))
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
    };
    let plan = build_plan(&info, &preset, &input).expect("build_plan failed");
    assert_eq!(plan.video_kbit, 0); // bitrate math doesn't apply to GIF
    assert_eq!(plan.audio_kbit, 0); // GIF never carries audio
    assert_eq!(
        plan.output,
        dir.path()
            .join(format!("clip (tamped {}).gif", preset_hash(&preset)))
    );

    let target_bytes = preset.target_mb * 1_000_000.0;
    let slot = ChildSlot::default();
    let mut seen: Vec<(u8, f64)> = Vec::new();
    run_gif(
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
    // generous target (and keeps the last file even when it can't — which
    // would fail this assertion and flag a regression).
    assert!(
        (bytes.len() as f64) <= target_bytes,
        "gif {} bytes exceeds {} byte target",
        bytes.len(),
        target_bytes
    );
}
