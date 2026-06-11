use std::path::{Path, PathBuf};

use crate::settings::{OutputFormat, Preset};

use super::probe::ProbeInfo;

#[derive(Clone, Debug)]
pub struct EncodePlan {
    pub video_kbit: u32,
    pub audio_kbit: u32,
    pub vf: Option<String>,
    pub output: PathBuf,
    pub format: OutputFormat,
    /// Palette-encode parameters; `Some` exactly when `format` is `Gif`.
    pub gif: Option<GifParams>,
    /// Frame-rate cap the planner imposed ON ITS OWN (replacing a higher —
    /// never a lower — user cap) because the target bitrate would starve the
    /// configured frame rate. `None` when the user's settings already fit.
    pub auto_fps: Option<u32>,
    /// Width the planner downscaled to ON ITS OWN, always below the user's
    /// own effective width (max_width / scale_percent stay ceilings) and
    /// never below [`AUTO_MIN_WIDTH`]. `None` when no auto-downscale ran.
    pub auto_width: Option<u32>,
    /// Bits per pixel per frame of the planned encode AFTER auto-degradation
    /// — the starvation metric the worker's VideoToolbox gate keys on.
    /// `f64::INFINITY` for GIF plans (not bitrate-targeted) and when the
    /// probe couldn't determine geometry/fps, so the gate never triggers on
    /// a guess.
    pub bpp: f64,
}

/// Bits per pixel per frame below which screen content stops being legible
/// for any encoder (and far below which VideoToolbox's rate control ignores
/// the requested bitrate entirely). When a plan lands under this floor the
/// planner degrades it: frame rate capped to [`AUTO_FPS_CAP`] first, then
/// the video is downscaled until the floor holds.
pub const BPP_FLOOR: f64 = 0.02;

/// The frame rate auto-degradation caps to before touching resolution.
/// A user `max_fps` BELOW this is always respected and never raised.
const AUTO_FPS_CAP: u32 = 30;

/// Auto-downscale never goes below this width (must stay even): narrower
/// screen recordings are unreadable, so past this point the plan proceeds
/// under [`BPP_FLOOR`] and the convergence loop is the backstop.
const AUTO_MIN_WIDTH: u32 = 640;

/// Bits per pixel per frame for `video_kbit` over a `pixels`-sized frame at
/// `fps` — the starvation metric behind the planner's auto-degradation and
/// the worker's VideoToolbox gate.
fn bits_per_pixel_frame(video_kbit: u32, pixels: f64, fps: f64) -> f64 {
    f64::from(video_kbit) * 1000.0 / (pixels * fps)
}

/// GIF encodes are palette-based, not bitrate-targeted: size is steered by
/// frame rate and width, which the worker shrinks iteratively when an
/// attempt overshoots the byte target.
#[derive(Clone, Copy, Debug)]
pub struct GifParams {
    pub fps: u32,
    pub max_width: u32,
}

pub fn build_plan(info: &ProbeInfo, preset: &Preset, input: &Path) -> Result<EncodePlan, String> {
    if info.duration_secs <= 0.0 {
        return Err("Video has no measurable duration".to_string());
    }

    let output = unique_output(input, &preset_hash(preset), preset.format);

    // GIF skips the bitrate math entirely (so a small target on a long clip
    // is not an error here) and never carries audio; fps/width live in the
    // palette filter graph instead of a -vf chain.
    if preset.format == OutputFormat::Gif {
        return Ok(EncodePlan {
            video_kbit: 0,
            audio_kbit: 0,
            vf: None,
            output,
            format: OutputFormat::Gif,
            gif: Some(GifParams {
                fps: preset.max_fps.unwrap_or(12),
                max_width: preset.max_width.unwrap_or(480),
            }),
            auto_fps: None,
            auto_width: None,
            bpp: f64::INFINITY,
        });
    }

    let target_bytes = preset.target_mb * 1_000_000.0;
    let budget_kbit = target_bytes * 8.0 / 1000.0 * 0.95; // 5% container margin
    let audio_kbit: u32 = if preset.strip_audio || !info.has_audio {
        0
    } else {
        match preset.format {
            OutputFormat::Webm => 64, // libopus
            _ => 96,                  // aac
        }
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

    // Automatic quality-floor degradation. A huge high-fps capture into a
    // small target can plan a bitrate that starves ANY encoder (a 3456x2234
    // @57fps recording into 10 MB plans ~0.001 bits/pixel/frame; VideoToolbox
    // ignores rates that low and libx264 refuses them outright), so when the
    // plan lands under BPP_FLOOR the frame rate is capped first, then the
    // video is downscaled until the floor holds — never above the user's own
    // fps/width caps, never below AUTO_MIN_WIDTH.
    let mut fps_eff = match preset.max_fps {
        Some(cap) => info.fps.min(f64::from(cap)),
        None => info.fps,
    };
    // Effective dimensions after the user's own scale filter, mirroring the
    // filter emission below. Heights are estimates — ffmpeg's `-2` rounding
    // can differ by a pixel, which is noise at bits-per-pixel scale.
    let (mut eff_w, mut eff_h) = if preset.max_width.is_some_and(|w| info.width > w) {
        let w = f64::from(preset.max_width.unwrap() & !1);
        (w, f64::from(info.height) * w / f64::from(info.width))
    } else if preset.scale_percent.is_some_and(|p| p != 100) {
        let w = f64::from((info.width * preset.scale_percent.unwrap() / 100) & !1);
        (w, f64::from(info.height) * w / f64::from(info.width))
    } else {
        (f64::from(info.width & !1), f64::from(info.height & !1))
    };
    let mut auto_fps: Option<u32> = None;
    let mut auto_width: Option<u32> = None;
    // Unknown geometry/fps stays INFINITY: neither the degradation here nor
    // the worker's hardware gate may ever trigger on a guess.
    let mut bpp = f64::INFINITY;
    if eff_w > 0.0 && eff_h > 0.0 && fps_eff > 0.0 {
        bpp = bits_per_pixel_frame(video_kbit, eff_w * eff_h, fps_eff);
        if bpp < BPP_FLOOR && fps_eff > f64::from(AUTO_FPS_CAP) {
            // A user max_fps at or below the cap already left fps_eff there
            // and stays untouched.
            fps_eff = f64::from(AUTO_FPS_CAP);
            auto_fps = Some(AUTO_FPS_CAP);
            bpp = bits_per_pixel_frame(video_kbit, eff_w * eff_h, fps_eff);
        }
        if bpp < BPP_FLOOR {
            let target_pixels = f64::from(video_kbit) * 1000.0 / (BPP_FLOOR * fps_eff);
            let aspect = eff_w / eff_h;
            let w = (((target_pixels * aspect).sqrt() as u32) & !1).max(AUTO_MIN_WIDTH);
            // Never upscale: when the width floor asks for >= the current
            // (user-capped) width there is nothing left to shrink — proceed
            // under the floor and let the convergence loop be the backstop.
            if f64::from(w) < eff_w {
                eff_h *= f64::from(w) / eff_w;
                eff_w = f64::from(w);
                auto_width = Some(w);
                bpp = bits_per_pixel_frame(video_kbit, eff_w * eff_h, fps_eff);
            }
        }
    }

    let mut filters: Vec<String> = Vec::new();
    if let Some(w) = auto_width {
        // The auto width REPLACES the user's scale filter: it is always
        // narrower than the user's own effective width, so max_width /
        // scale_percent remain ceilings, and min(iw,…) keeps the
        // never-upscale guarantee.
        filters.push(format!("scale='trunc(min(iw,{w})/2)*2':-2"));
    } else if preset.max_width.is_some_and(|w| info.width > w) {
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
    if let Some(fps) = auto_fps {
        filters.push(format!("fps={fps}"));
    } else if preset.max_fps.is_some_and(|f| info.fps > f as f64) {
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
        output,
        format: preset.format,
        gif: None,
        auto_fps,
        auto_width,
        bpp,
    })
}

/// The output file extension per format. Naming, reuse probing and the
/// frontend all derive the extension from here via the plan's output path.
fn extension(format: OutputFormat) -> &'static str {
    match format {
        OutputFormat::Mp4 => "mp4",
        OutputFormat::Webm => "webm",
        OutputFormat::Gif => "gif",
    }
}

/// The palettegen/paletteuse filter graph for one GIF attempt: cap the frame
/// rate, scale down to at most `max_width` (kept even, never upscaling),
/// then build a per-clip palette and dither against it.
pub fn gif_filter(fps: u32, max_width: u32) -> String {
    format!(
        "[0:v]fps={fps},scale='trunc(min(iw,{max_width})/2)*2':-2[s];\
         [s]split[a][b];\
         [a]palettegen=stats_mode=diff[p];\
         [b][p]paletteuse=dither=bayer:bayer_scale=4"
    )
}

/// Next GIF width to try when an attempt overshoots the byte target: GIF
/// bytes scale roughly with pixel area, so scale the width by the square
/// root of the size ratio with a 5% safety margin, truncated to even and
/// floored at 160 so retries can't degenerate into unreadable thumbnails.
/// The floor never RAISES a width above `initial_width` — a preset that
/// starts narrower than 160 simply holds at its own starting width.
pub fn gif_retry_width(
    width: u32,
    initial_width: u32,
    target_bytes: f64,
    actual_bytes: u64,
) -> u32 {
    let scaled = (width as f64 * (target_bytes / actual_bytes as f64).sqrt() * 0.95) as u32;
    (scaled & !1).max(160.min(initial_width))
}

/// Parameters for the GIF retry numbered `retry_index` (1-based: the first
/// re-encode after the initial attempt is retry 1). Every retry shrinks the
/// width via [`gif_retry_width`]; from the second retry — the THIRD attempt
/// overall — the frame rate also drops to 3/4, floored at 8 fps, because
/// width shrinkage alone has clearly not been enough by then. `initial` is
/// the plan's STARTING params: both floors clamp to it so a retry can never
/// raise fps or width above what the user's own preset asked for.
pub fn gif_retry_params(
    current: GifParams,
    initial: GifParams,
    retry_index: u8,
    target_bytes: f64,
    actual_bytes: u64,
) -> GifParams {
    GifParams {
        fps: if retry_index >= 2 {
            (current.fps * 3 / 4).max(8.min(initial.fps))
        } else {
            current.fps
        },
        max_width: gif_retry_width(
            current.max_width,
            initial.max_width,
            target_bytes,
            actual_bytes,
        ),
    }
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
/// Non-mp4 formats fold their name in as a FINAL step so every mp4 preset —
/// including all pre-format outputs in the wild — hashes exactly as before.
/// Cosmetic fields (id, name) are deliberately excluded.
pub fn preset_hash(p: &crate::settings::Preset) -> String {
    let mut h = fnv1a(FNV_OFFSET, &p.target_mb.to_bits().to_le_bytes());
    h = fnv1a(h, &p.max_fps.unwrap_or(u32::MAX).to_le_bytes());
    h = fnv1a(h, &p.max_width.unwrap_or(u32::MAX).to_le_bytes());
    h = fnv1a(h, &p.scale_percent.unwrap_or(u32::MAX).to_le_bytes());
    h = fnv1a(h, &[u8::from(p.strip_audio)]);
    match p.format {
        OutputFormat::Mp4 => {}
        OutputFormat::Webm => h = fnv1a(h, b"webm"),
        OutputFormat::Gif => h = fnv1a(h, b"gif"),
    }
    format!("{:04x}", h & 0xffff)
}

fn input_stem(input: &Path) -> String {
    input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "video".to_string())
}

/// The collision-free base output path for `input` under a preset `hash`:
/// "{stem} (tamped {hash}).mp4|.webm|.gif" next to the input. The worker
/// probes this exact path before encoding — when it already exists the
/// previous output is reused instead of re-encoding.
pub fn expected_output(input: &Path, hash: &str, format: OutputFormat) -> PathBuf {
    let dir = input.parent().unwrap_or_else(|| Path::new("."));
    let ext = extension(format);
    dir.join(format!("{} (tamped {hash}).{ext}", input_stem(input)))
}

/// The crash-safe temp sibling every encode attempt writes to:
/// "{final_stem}.{ext}.part" in the SAME directory as `output`, so the final
/// rename never crosses filesystems. Its extension is "part", which the
/// recents scanner never lists, so a crash can only ever leave a `.part`
/// file behind — never a partial at the final output path.
pub fn part_path(output: &Path) -> PathBuf {
    let mut name = output
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".part");
    output.with_file_name(name)
}

/// True when `name` is a numbered sibling of the base output whose stem is
/// `base_stem` (e.g. "clip (tamped 823f)") with extension `ext`: exactly
/// "{stem} (tamped {hash} N).{ext}" for some run of digits N. The base
/// output itself, other hashes/stems/extensions and `.part` files all fail.
pub fn is_numbered_sibling(name: &str, base_stem: &str, ext: &str) -> bool {
    let Some(stem) = name.strip_suffix(ext).and_then(|n| n.strip_suffix('.')) else {
        return false;
    };
    let Some(base) = base_stem.strip_suffix(')') else {
        return false;
    };
    let Some(digits) = stem
        .strip_prefix(base)
        .and_then(|r| r.strip_prefix(' '))
        .and_then(|r| r.strip_suffix(')'))
    else {
        return false;
    };
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

fn unique_output(input: &Path, hash: &str, format: OutputFormat) -> PathBuf {
    let candidate = expected_output(input, hash, format);
    if !candidate.exists() {
        return candidate;
    }
    let dir = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input_stem(input);
    let ext = extension(format);
    for n in 2u32.. {
        let candidate = dir.join(format!("{stem} (tamped {hash} {n}).{ext}"));
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
            format: OutputFormat::Mp4,
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
        // 1266 kbit over 1920x1080 at the 60 fps source is ~0.010 bpp — under
        // the quality floor — so the planner auto-caps to 30 fps (which alone
        // clears the floor: ~0.020 bpp, no downscale needed).
        assert_eq!(plan.auto_fps, Some(30));
        assert_eq!(plan.auto_width, None);
        assert_eq!(
            plan.vf.as_deref(),
            Some("scale=trunc(iw/2)*2:trunc(ih/2)*2,fps=30,format=yuv420p")
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

        // 30 fps so the bpp floor stays out of this test's way.
        let narrow = ProbeInfo {
            width: 1280,
            fps: 30.0,
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
        // 30 fps so the bpp floor stays out of this test's way.
        let plan = build_plan(
            &ProbeInfo {
                fps: 30.0,
                ..info()
            },
            &p,
            Path::new(INPUT),
        )
        .unwrap();
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

    /// Sparse file of `bytes` length: build_plan only stats the size, so the
    /// 491 MB production input costs no real disk.
    fn sparse_input(dir: &Path, name: &str, bytes: u64) -> PathBuf {
        let path = dir.join(name);
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(bytes).unwrap();
        path
    }

    /// The probe of the real-world starved recording: 3456x2234 @ ~57 fps,
    /// 127.78 s, with audio.
    fn starved_info() -> ProbeInfo {
        ProbeInfo {
            duration_secs: 127.78,
            width: 3456,
            height: 2234,
            fps: 57.0,
            has_audio: true,
        }
    }

    #[test]
    fn starved_production_case_auto_caps_fps_and_downscales() {
        // The production bug: a 491 MB 3456x2234@57 screen recording into a
        // 10 MB target plans 498 kbit — 0.0011 bits/pixel/frame, starvation
        // for ANY encoder (VideoToolbox ignored the rate and emitted 89.7 MB;
        // libx264 refused the post-overshoot correction outright). The
        // planner must cap to 30 fps, then downscale until BPP_FLOOR holds —
        // empirically ~1132px, which fit the same file in 9.71 MB.
        let dir = tempfile::tempdir().unwrap();
        let input = sparse_input(dir.path(), "screen.mov", 491_014_761);
        let plan = build_plan(&starved_info(), &preset(10.0), &input).unwrap();
        assert_eq!(plan.video_kbit, 498); // 76000/127.78 - 96
        assert_eq!(plan.auto_fps, Some(30));
        let w = plan.auto_width.expect("starved plan must auto-downscale");
        assert!((1100..=1200).contains(&w), "auto width {w}");
        assert_eq!(w % 2, 0, "auto width must be even: {w}");
        assert_eq!(
            plan.vf.as_deref(),
            Some(format!("scale='trunc(min(iw,{w})/2)*2':-2,fps=30,format=yuv420p").as_str())
        );
        assert!(plan.bpp >= 0.019, "bpp {}", plan.bpp);
    }

    #[test]
    fn unstarved_plan_gets_no_auto_degradation() {
        // 1920x1080@30 over 60 s into 10 MB is 1266 kbit ≈ 0.020 bpp — at the
        // floor, so nothing is auto-applied and the vf chain is untouched.
        let info = ProbeInfo {
            fps: 30.0,
            ..info()
        };
        let plan = build_plan(&info, &preset(10.0), Path::new(INPUT)).unwrap();
        assert_eq!(plan.auto_fps, None);
        assert_eq!(plan.auto_width, None);
        assert_eq!(
            plan.vf.as_deref(),
            Some("scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p")
        );
        assert!(plan.bpp >= BPP_FLOOR, "bpp {}", plan.bpp);
    }

    #[test]
    fn auto_fps_never_raises_a_lower_user_cap() {
        // A user max_fps of 24 is BELOW the 30 fps auto-cap: the starved plan
        // must keep fps=24 (auto_fps stays None) and degrade via width only.
        let dir = tempfile::tempdir().unwrap();
        let input = sparse_input(dir.path(), "screen.mov", 491_014_761);
        let mut p = preset(10.0);
        p.max_fps = Some(24);
        let plan = build_plan(&starved_info(), &p, &input).unwrap();
        assert_eq!(plan.auto_fps, None, "a user cap below 30 stays untouched");
        let w = plan.auto_width.expect("still starved at 24 fps");
        assert_eq!(
            plan.vf.as_deref(),
            Some(format!("scale='trunc(min(iw,{w})/2)*2':-2,fps=24,format=yuv420p").as_str())
        );
        assert!(plan.bpp >= 0.019, "bpp {}", plan.bpp);
    }

    #[test]
    fn user_max_width_stays_the_ceiling_for_auto_degradation() {
        // Without a user cap the production case auto-downscales to ~1132px.
        // With max_width 800 the user's own scaling already clears the floor
        // at the source frame rate — nothing may raise the width back up.
        let dir = tempfile::tempdir().unwrap();
        let input = sparse_input(dir.path(), "screen.mov", 491_014_761);
        let mut p = preset(10.0);
        p.max_width = Some(800);
        let plan = build_plan(&starved_info(), &p, &input).unwrap();
        assert_eq!(plan.auto_fps, None);
        assert_eq!(plan.auto_width, None);
        assert_eq!(
            plan.vf.as_deref(),
            Some("scale='trunc(min(iw,800)/2)*2':-2,format=yuv420p")
        );
        assert!(plan.bpp >= BPP_FLOOR, "bpp {}", plan.bpp);

        // Still starved UNDER the user's cap (5 MB target): the auto width
        // must land below the 800px ceiling, never above it.
        let mut p5 = preset(5.0);
        p5.max_width = Some(800);
        let plan = build_plan(&starved_info(), &p5, &input).unwrap();
        assert_eq!(plan.auto_fps, Some(30));
        let w = plan.auto_width.expect("still starved at 800px");
        assert!((640..800).contains(&w), "auto width {w}");
        assert_eq!(w % 2, 0, "auto width must be even: {w}");
    }

    #[test]
    fn auto_width_floors_at_640() {
        // A long square recording plans 126 kbit; the ideal width (~458px)
        // falls below the floor, so the planner holds at 640 even though the
        // bpp stays under BPP_FLOOR — the convergence loop is the backstop.
        let info = ProbeInfo {
            duration_secs: 600.0,
            width: 2160,
            height: 2160,
            fps: 30.0,
            has_audio: false,
        };
        let plan = build_plan(&info, &preset(10.0), Path::new(INPUT)).unwrap();
        assert_eq!(plan.auto_fps, None); // source is already at 30 fps
        assert_eq!(plan.auto_width, Some(640));
        assert_eq!(
            plan.vf.as_deref(),
            Some("scale='trunc(min(iw,640)/2)*2':-2,format=yuv420p")
        );
        assert!(plan.bpp < BPP_FLOOR, "bpp {}", plan.bpp); // proceed anyway
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
            expected_output(Path::new(INPUT), "823f", OutputFormat::Mp4),
            PathBuf::from("/nonexistent-tamp-test/clip (tamped 823f).mp4")
        );
    }

    #[test]
    fn output_extension_follows_format() {
        assert_eq!(
            expected_output(Path::new(INPUT), "8dd6", OutputFormat::Webm),
            PathBuf::from("/nonexistent-tamp-test/clip (tamped 8dd6).webm")
        );
        assert_eq!(
            expected_output(Path::new(INPUT), "270f", OutputFormat::Gif),
            PathBuf::from("/nonexistent-tamp-test/clip (tamped 270f).gif")
        );

        let mut webm = preset(10.0);
        webm.format = OutputFormat::Webm;
        let plan = build_plan(&info(), &webm, Path::new(INPUT)).unwrap();
        assert_eq!(
            plan.output,
            PathBuf::from("/nonexistent-tamp-test/clip (tamped 8dd6).webm")
        );

        let mut gif = preset(10.0);
        gif.format = OutputFormat::Gif;
        let plan = build_plan(&info(), &gif, Path::new(INPUT)).unwrap();
        assert_eq!(
            plan.output,
            PathBuf::from("/nonexistent-tamp-test/clip (tamped 270f).gif")
        );
    }

    #[test]
    fn webm_audio_budget_is_64k() {
        // 1280x720@30 keeps the bpp floor out of this test's way (full-HD at
        // 1202 kbit would trigger the auto-degradation).
        let info = ProbeInfo {
            has_audio: true,
            width: 1280,
            height: 720,
            fps: 30.0,
            ..info()
        };
        let mut p = preset(10.0);
        p.format = OutputFormat::Webm;
        let plan = build_plan(&info, &p, Path::new(INPUT)).unwrap();
        assert_eq!(plan.audio_kbit, 64);
        assert_eq!(plan.video_kbit, 1202); // 1266.66 - 64
        assert_eq!(plan.format, OutputFormat::Webm);
        assert!(plan.gif.is_none());
        // vf chain is unchanged for vp9
        assert_eq!(
            plan.vf.as_deref(),
            Some("scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p")
        );
    }

    #[test]
    fn gif_plan_skips_bitrate_math_and_audio() {
        // A target far too small for the bitrate math: must NOT error for GIF.
        let info = ProbeInfo {
            duration_secs: 600.0,
            has_audio: true,
            ..info()
        };
        let mut p = preset(0.1);
        p.format = OutputFormat::Gif;
        let plan = build_plan(&info, &p, Path::new(INPUT)).unwrap();
        assert_eq!(plan.video_kbit, 0);
        assert_eq!(plan.audio_kbit, 0); // GIF never carries audio
        assert!(plan.vf.is_none()); // fps/width live in the palette graph
        let gif = plan.gif.expect("gif plan must carry palette params");
        assert_eq!(gif.fps, 12); // defaults
        assert_eq!(gif.max_width, 480);
    }

    #[test]
    fn gif_params_honour_preset_caps() {
        let mut p = preset(10.0);
        p.format = OutputFormat::Gif;
        p.max_fps = Some(15);
        p.max_width = Some(640);
        let plan = build_plan(&info(), &p, Path::new(INPUT)).unwrap();
        let gif = plan.gif.unwrap();
        assert_eq!(gif.fps, 15);
        assert_eq!(gif.max_width, 640);
    }

    #[test]
    fn gif_filter_caps_fps_and_width() {
        assert_eq!(
            gif_filter(12, 480),
            "[0:v]fps=12,scale='trunc(min(iw,480)/2)*2':-2[s];\
             [s]split[a][b];\
             [a]palettegen=stats_mode=diff[p];\
             [b][p]paletteuse=dither=bayer:bayer_scale=4"
        );
    }

    #[test]
    fn gif_retry_width_shrinks_by_sqrt_size_ratio() {
        // 4x over target: sqrt(1/4) * 0.95 = 0.475 -> 480 * 0.475 = 228
        assert_eq!(gif_retry_width(480, 480, 1_000_000.0, 4_000_000), 228);
        // Slightly over: 480 * sqrt(1/1.1) * 0.95 = 434.8 -> 434 (even)
        assert_eq!(gif_retry_width(480, 480, 1_000_000.0, 1_100_000), 434);
        // Odd results truncate to even: 300 * sqrt(0.5) * 0.95 = 201.5 -> 200
        assert_eq!(gif_retry_width(300, 300, 1_000_000.0, 2_000_000), 200);
        // Never below the 160px floor.
        assert_eq!(gif_retry_width(200, 200, 100.0, 1_000_000_000), 160);
    }

    #[test]
    fn gif_retry_width_floor_never_raises_a_narrow_start() {
        // A preset narrower than the 160px floor holds at ITS OWN width — a
        // retry must never come out wider than the user's starting params.
        assert_eq!(gif_retry_width(120, 120, 100.0, 1_000_000_000), 120);
        // Even a mild overshoot holds there: the start is already below the
        // readability floor, so fps becomes the only remaining knob.
        assert_eq!(gif_retry_width(120, 120, 1_000_000.0, 1_100_000), 120);
        // A wide start keeps the standard 160px floor.
        assert_eq!(gif_retry_width(480, 480, 100.0, 1_000_000_000), 160);
    }

    const INITIAL: GifParams = GifParams {
        fps: 12,
        max_width: 480,
    };

    #[test]
    fn gif_retry_params_first_retry_shrinks_width_only() {
        // 4x over target: width follows gif_retry_width, fps untouched.
        let next = gif_retry_params(INITIAL, INITIAL, 1, 1_000_000.0, 4_000_000);
        assert_eq!(
            next.max_width,
            gif_retry_width(480, 480, 1_000_000.0, 4_000_000)
        );
        assert_eq!(next.fps, 12);
    }

    #[test]
    fn gif_retry_params_reduces_fps_from_second_retry() {
        for retry in [2u8, 3, 4] {
            let next = gif_retry_params(INITIAL, INITIAL, retry, 1_000_000.0, 4_000_000);
            assert_eq!(next.fps, 9, "retry {retry}"); // 12 * 3/4
        }
        // Compounding across retries: 12 -> 9 -> 8 (6.75 floors to 8) -> 8.
        let mut p = INITIAL;
        let mut seen = Vec::new();
        for retry in 2u8..=4 {
            p = gif_retry_params(p, INITIAL, retry, 1_000_000.0, 4_000_000);
            seen.push(p.fps);
        }
        assert_eq!(seen, vec![9, 8, 8]);
    }

    #[test]
    fn gif_retry_params_respects_both_floors() {
        let current = GifParams {
            fps: 8,
            max_width: 160,
        };
        // Hugely over target with both knobs already at their floors: the
        // schedule must hold at 160px / 8fps, never below.
        let next = gif_retry_params(current, INITIAL, 4, 100.0, 1_000_000_000);
        assert_eq!(next.max_width, 160);
        assert_eq!(next.fps, 8);
    }

    #[test]
    fn gif_retry_params_never_raise_a_low_fps_start() {
        // A max_fps 5 preset starts at 5 fps; the 8 fps floor must clamp to
        // the user's own start, so every retry stays <= 5 — never raised.
        let initial = GifParams {
            fps: 5,
            max_width: 480,
        };
        let mut p = initial;
        for retry in 1u8..=4 {
            p = gif_retry_params(p, initial, retry, 1_000_000.0, 4_000_000);
            assert!(p.fps <= 5, "retry {retry} raised fps to {}", p.fps);
        }
        assert_eq!(p.fps, 5); // 5 * 3/4 = 3 floors back up to min(8, 5) = 5
    }

    #[test]
    fn gif_retry_params_never_raise_a_narrow_width_start() {
        // A 120px preset must hold at 120, not get raised to the 160 floor.
        let initial = GifParams {
            fps: 12,
            max_width: 120,
        };
        let next = gif_retry_params(initial, initial, 1, 100.0, 1_000_000_000);
        assert_eq!(next.max_width, 120);
        // Both knobs at their clamped floors: the params stop changing, which
        // is exactly what run_gif's stuck check keys on to give up.
        let stuck = GifParams {
            fps: 5,
            max_width: 120,
        };
        let initial = stuck;
        let next = gif_retry_params(stuck, initial, 2, 100.0, 1_000_000_000);
        assert_eq!(next.fps, stuck.fps);
        assert_eq!(next.max_width, stuck.max_width);
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
            format: OutputFormat::Mp4,
        };
        assert_eq!(preset_hash(&full), "eb3d");
    }

    // Pinned like the mp4 hashes above: computed with a reference FNV-1a
    // implementation, must never change across releases.
    #[test]
    fn preset_hash_folds_non_mp4_formats_as_final_step() {
        let mut webm = preset(10.0);
        webm.format = OutputFormat::Webm;
        assert_eq!(preset_hash(&webm), "8dd6");

        let mut gif = preset(10.0);
        gif.format = OutputFormat::Gif;
        assert_eq!(preset_hash(&gif), "270f");

        // mp4 must hash exactly as before formats existed.
        assert_eq!(preset_hash(&preset(10.0)), "823f");
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
    fn part_path_appends_part_in_the_same_directory() {
        assert_eq!(
            part_path(Path::new("/dir/clip (tamped 823f).mp4")),
            PathBuf::from("/dir/clip (tamped 823f).mp4.part")
        );
        assert_eq!(
            part_path(Path::new("/dir/clip (tamped 270f 2).gif")),
            PathBuf::from("/dir/clip (tamped 270f 2).gif.part")
        );
        // The scanner must never list a part file as a video.
        assert_eq!(
            part_path(Path::new("/dir/c.webm")).extension().unwrap(),
            "part"
        );
    }

    #[test]
    fn matches_numbered_siblings_of_the_exact_base_output() {
        let base = "clip (tamped 823f)";
        assert!(is_numbered_sibling("clip (tamped 823f 2).mp4", base, "mp4"));
        assert!(is_numbered_sibling(
            "clip (tamped 823f 17).mp4",
            base,
            "mp4"
        ));
        for name in [
            "clip (tamped 823f).mp4",        // the base output itself
            "clip (tamped 823f 2).gif",      // wrong extension
            "clip (tamped ffff 2).mp4",      // different hash
            "other (tamped 823f 2).mp4",     // different stem
            "clip (tamped 823f x).mp4",      // counter must be digits
            "clip (tamped 823f ).mp4",       // empty counter
            "clip (tamped 823f 2).mp4.part", // in-flight temp, never swept
            "clip (tamped 823f 2 3).mp4",    // extra token
        ] {
            assert!(!is_numbered_sibling(name, base, "mp4"), "{name}");
        }
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
