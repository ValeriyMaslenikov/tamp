//! Which hardware H.264 encoder to try: the platform names its candidates
//! in preference order; this module filters them against what the bundled
//! ffmpeg actually ships (`-encoders`), once per process.

use crate::platform::{HwCandidate, Platform as _};
use tokio::sync::OnceCell;

static AVAILABLE: OnceCell<Vec<&'static HwCandidate>> = OnceCell::const_new();

/// The platform's hardware candidates that the bundled ffmpeg supports, in
/// preference order. Probed once and cached; an empty slice means every MP4
/// encode goes straight to two-pass software.
pub async fn available_candidates() -> &'static [&'static HwCandidate] {
    AVAILABLE
        .get_or_init(|| async {
            let names = match encoder_list().await {
                Ok(names) => names,
                Err(e) => {
                    crate::log_warn!(
                        "cannot probe ffmpeg encoders ({e}); hardware encoding disabled"
                    );
                    return Vec::new();
                }
            };
            let available: Vec<&'static HwCandidate> = crate::platform::native()
                .hw_candidates()
                .iter()
                .filter(|c| names.contains(c.name))
                .collect();
            crate::log_info!(
                "hardware encoder candidates: [{}]",
                available
                    .iter()
                    .map(|c| c.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            available
        })
        .await
}

async fn encoder_list() -> Result<String, String> {
    let out = crate::platform::background_command(super::bin::ffmpeg_path())
        .args(["-hide_banner", "-encoders"])
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|e| format!("failed to run ffmpeg -encoders: {e}"))?;
    if !out.status.success() {
        return Err(format!("ffmpeg -encoders exited with {}", out.status));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
