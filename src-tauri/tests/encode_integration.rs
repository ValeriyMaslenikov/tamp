//! End-to-end exercise of probe -> plan -> two-pass encode using the bundled
//! ffmpeg binaries (src-tauri/binaries). No AppHandle needed: it drives the
//! same `run_passes` pipeline the worker uses.

use std::path::{Path, PathBuf};
use std::process::Command;

use tamp_lib::encoder::{bin, plan::build_plan, probe::probe, run_passes, ChildSlot, Preset};

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
            "-pix_fmt",
            "yuv420p",
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
    };
    let plan = build_plan(&info, &preset, &input).expect("build_plan failed");
    assert_eq!(plan.audio_kbit, 96);
    assert_eq!(plan.vf, None);
    assert!(plan.video_kbit >= 100);
    assert_eq!(plan.output, dir.path().join("clip (tamped).mp4"));

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
}
