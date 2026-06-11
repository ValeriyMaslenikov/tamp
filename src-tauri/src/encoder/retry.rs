//! Size-convergence retry policy for bitrate-targeted encodes (mp4/webm).
//!
//! A Done job's output must NEVER exceed its byte target (Discord rejects a
//! file even one byte over the limit), so after every attempt the worker asks
//! [`next_action`] what to do: deliver the file, re-encode with a corrected
//! bitrate (switching a VideoToolbox job to two-pass software), or give up
//! and fail the job with an actionable error instead of delivering an
//! oversized file.

/// Which encoder produced an attempt (and which one should run next).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncoderKind {
    /// Single-pass `h264_videotoolbox`.
    Hardware,
    /// Two-pass software (libx264 / libvpx-vp9).
    Software,
}

/// The decision for what to do after an encode attempt lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NextAction {
    /// The output fits the byte target: deliver it.
    Done,
    /// Re-encode with the corrected bitrate on the given encoder.
    Retry {
        video_kbit: u32,
        encoder: EncoderKind,
    },
    /// Bounded retries exhausted, or a retry would need an unwatchably low
    /// bitrate: fail the job with [`give_up_error`].
    GiveUp,
}

/// Re-encodes allowed after the initial attempt.
pub const MAX_RE_ENCODES: u8 = 3;

/// Bitrate below which a retry can't produce watchable video — give up
/// instead of grinding out unusable output.
pub const MIN_VIDEO_KBIT: u32 = 50;

/// Per-re-encode safety shrink applied on top of the proportional
/// `target/actual` correction for SOFTWARE overshoots. Each successive
/// re-encode tightens further so a stubborn overshoot converges instead of
/// oscillating around the target.
const SHRINK: [f64; MAX_RE_ENCODES as usize] = [0.95, 0.90, 0.85];

/// Decides what to do after the attempt numbered `attempt_index` (0 is the
/// initial encode) produced `actual_bytes` at `video_kbit` on `encoder`.
///
/// A HARDWARE overshoot retries software at `video_kbit * 0.95`, NOT at an
/// actual-derived correction: hardware only ever runs the planned bitrate,
/// and VideoToolbox overshooting means its quality floor IGNORED the request
/// (a 498 kbit request once came out at 89.7 MB) — which says nothing about
/// how two-pass software will track the same rate, while the actual-derived
/// correction (52 kbit there) is one libx264 refuses outright. Software
/// overshoots are real, so they keep the proportional `target/actual`
/// correction with the compounding shrink schedule.
pub fn next_action(
    video_kbit: u32,
    actual_bytes: u64,
    target_bytes: f64,
    attempt_index: u8,
    encoder: EncoderKind,
) -> NextAction {
    if actual_bytes as f64 <= target_bytes {
        return NextAction::Done;
    }
    if attempt_index >= MAX_RE_ENCODES {
        return NextAction::GiveUp;
    }
    let corrected = match encoder {
        EncoderKind::Hardware => (video_kbit as f64 * 0.95) as u32,
        EncoderKind::Software => {
            let shrink = SHRINK[attempt_index as usize];
            (video_kbit as f64 * (target_bytes / actual_bytes as f64) * shrink) as u32
        }
    };
    if corrected < MIN_VIDEO_KBIT {
        return NextAction::GiveUp;
    }
    NextAction::Retry {
        video_kbit: corrected,
        // Every retry runs in software: x264/vp9 two-pass ABR tracks the
        // requested bitrate far more accurately than VideoToolbox's
        // single-pass rate control, so a hardware job switches on its FIRST
        // overshoot and a software job simply tightens its bitrate.
        encoder: EncoderKind::Software,
    }
}

/// The actionable failure shown when bounded retries can't fit the target.
pub fn give_up_error(target_mb: f64) -> String {
    // Render "8 MB", not "8.0 MB", for whole-number targets.
    let n = if target_mb.fract() == 0.0 {
        format!("{}", target_mb as i64)
    } else {
        format!("{target_mb}")
    };
    format!("Couldn't fit under {n} MB — lower the resolution/FPS in the preset or pick a bigger target")
}

#[cfg(test)]
mod tests {
    use super::*;

    const TARGET: f64 = 1_000_000.0;

    #[test]
    fn fitting_output_is_done_immediately() {
        assert_eq!(
            next_action(1000, 999_999, TARGET, 0, EncoderKind::Hardware),
            NextAction::Done
        );
        // Exactly at the target is a fit — the guarantee is <=, not <.
        assert_eq!(
            next_action(1000, 1_000_000, TARGET, 0, EncoderKind::Software),
            NextAction::Done
        );
        // A fit on the last allowed attempt is still Done, never GiveUp.
        assert_eq!(
            next_action(1000, 900_000, TARGET, MAX_RE_ENCODES, EncoderKind::Software),
            NextAction::Done
        );
    }

    #[test]
    fn shrink_schedule_is_95_90_85() {
        // 1.25x over target: proportional correction is 0.8.
        for (attempt, shrink) in [(0u8, 0.95), (1, 0.90), (2, 0.85)] {
            let expected = (1000.0 * 0.8 * shrink) as u32;
            assert_eq!(
                next_action(1000, 1_250_000, TARGET, attempt, EncoderKind::Software),
                NextAction::Retry {
                    video_kbit: expected,
                    encoder: EncoderKind::Software
                },
                "attempt {attempt}"
            );
        }
    }

    #[test]
    fn hardware_overshoot_retries_at_the_planned_bitrate_times_095() {
        // The production bug: VideoToolbox ignored a 498 kbit request and
        // emitted 89.7 MB against a 10 MB target. The actual-derived
        // correction (498 * 10/89.7 * 0.95 = 52 kbit) is meaningless —
        // hardware ignoring the rate says nothing about x264, and libx264
        // refuses bitrates that low ("estimated minimum is 73 kbps"). The
        // switch must hand software the PLANNED bitrate * 0.95 instead.
        assert_eq!(
            next_action(498, 89_700_000, 10_000_000.0, 0, EncoderKind::Hardware),
            NextAction::Retry {
                video_kbit: 473, // 498 * 0.95
                encoder: EncoderKind::Software
            }
        );
        // The 50 kbit floor still applies on the switch (50 * 0.95 = 47).
        assert_eq!(
            next_action(50, 89_700_000, 10_000_000.0, 0, EncoderKind::Hardware),
            NextAction::GiveUp
        );
    }

    #[test]
    fn software_overshoot_keeps_the_actual_derived_correction() {
        // The same numbers from SOFTWARE mean the encoder genuinely emitted
        // 9x the target, so the proportional correction stands.
        assert_eq!(
            next_action(498, 89_700_000, 10_000_000.0, 0, EncoderKind::Software),
            NextAction::Retry {
                video_kbit: 52, // 498 * (10/89.7) * 0.95
                encoder: EncoderKind::Software
            }
        );
    }

    #[test]
    fn hardware_overshoot_switches_to_software_for_all_retries() {
        // The FIRST VideoToolbox overshoot must hand the job to two-pass
        // software; software jobs stay software and just tighten.
        for attempt in 0..MAX_RE_ENCODES {
            for kind in [EncoderKind::Hardware, EncoderKind::Software] {
                match next_action(1000, 1_100_000, TARGET, attempt, kind) {
                    NextAction::Retry { encoder, .. } => {
                        assert_eq!(encoder, EncoderKind::Software, "{kind:?} attempt {attempt}")
                    }
                    other => panic!("expected Retry, got {other:?}"),
                }
            }
        }
    }

    #[test]
    fn gives_up_after_bounded_re_encodes() {
        assert_eq!(
            next_action(
                1000,
                1_100_000,
                TARGET,
                MAX_RE_ENCODES,
                EncoderKind::Software
            ),
            NextAction::GiveUp
        );
        assert_eq!(
            next_action(
                1000,
                1_100_000,
                TARGET,
                MAX_RE_ENCODES + 1,
                EncoderKind::Hardware
            ),
            NextAction::GiveUp
        );
    }

    #[test]
    fn gives_up_below_the_bitrate_floor() {
        // 60 kbit at 10x over target corrects to ~5 kbit — unwatchable, so
        // the controller gives up instead of burning retries.
        assert_eq!(
            next_action(60, 10_000_000, TARGET, 0, EncoderKind::Software),
            NextAction::GiveUp
        );
        // At or above the floor is still allowed: 120 kbit, 2x over ->
        // 120 * 0.5 * 0.95 = 57.
        assert_eq!(
            next_action(120, 2_000_000, TARGET, 0, EncoderKind::Software),
            NextAction::Retry {
                video_kbit: 57,
                encoder: EncoderKind::Software
            }
        );
    }

    /// Drives the controller against a mocked encoder model, returning
    /// (re-encodes performed, converged?).
    fn simulate(initial_kbit: u32, model: impl Fn(u32) -> u64) -> (u8, bool) {
        let mut kbit = initial_kbit;
        let mut encoder = EncoderKind::Hardware;
        for attempt in 0..=MAX_RE_ENCODES {
            let actual = model(kbit);
            match next_action(kbit, actual, TARGET, attempt, encoder) {
                NextAction::Done => return (attempt, true),
                NextAction::Retry {
                    video_kbit,
                    encoder: next,
                } => {
                    kbit = video_kbit;
                    encoder = next;
                }
                NextAction::GiveUp => return (attempt, false),
            }
        }
        unreachable!("next_action gives up at MAX_RE_ENCODES")
    }

    #[test]
    fn converges_quickly_for_realistic_overshoots() {
        let initial = 2000u32;
        for ratio in [1.01, 1.05, 1.1, 1.2, 1.35, 1.5] {
            // Linear model: output bytes proportional to the bitrate (what a
            // working ABR encoder does); initial attempt is `ratio`x over.
            let (re_encodes, converged) = simulate(initial, |kbit| {
                (kbit as f64 / initial as f64 * ratio * TARGET) as u64
            });
            assert!(converged, "ratio {ratio} never converged");
            assert!(
                re_encodes <= 3,
                "ratio {ratio} took {re_encodes} re-encodes"
            );
        }
    }

    #[test]
    fn converges_even_when_the_encoder_under_responds() {
        // Pathological model: halving the bitrate only shrinks the file by
        // sqrt(2) — far worse than any real encoder. The compounding shrink
        // schedule must still land under target within the bounded retries.
        let initial = 2000u32;
        let (re_encodes, converged) = simulate(initial, |kbit| {
            ((kbit as f64 / initial as f64).sqrt() * 1.5 * TARGET) as u64
        });
        assert!(converged, "sticky encoder never converged");
        assert!(re_encodes <= 3, "took {re_encodes} re-encodes");
    }

    #[test]
    fn gives_up_when_the_overshoot_never_responds() {
        // Broken-encoder model: output is 2x over target no matter the
        // bitrate. The controller must stop after MAX_RE_ENCODES re-encodes.
        let (re_encodes, converged) = simulate(2000, |_| (2.0 * TARGET) as u64);
        assert!(!converged);
        assert_eq!(re_encodes, MAX_RE_ENCODES);
    }

    #[test]
    fn give_up_error_is_actionable_and_formats_whole_megabytes() {
        let err = give_up_error(8.0);
        assert_eq!(
            err,
            "Couldn't fit under 8 MB — lower the resolution/FPS in the preset or pick a bigger target"
        );
        assert!(give_up_error(0.5).contains("under 0.5 MB"));
    }
}
