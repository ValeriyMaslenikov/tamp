use std::path::PathBuf;

fn resolve(name: &str) -> PathBuf {
    if cfg!(debug_assertions) {
        // Dev builds run from target/, so reach into the repo's binaries dir.
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries").join(format!(
            "{name}-{}{}",
            env!("TAMP_TARGET_TRIPLE"),
            std::env::consts::EXE_SUFFIX
        ))
    } else {
        // Tauri bundles externalBin next to the main binary, stripped of the
        // triple (keeping the platform's exe suffix).
        let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join(&file)))
            .unwrap_or_else(|| PathBuf::from(file))
    }
}

pub fn ffmpeg_path() -> PathBuf {
    resolve("ffmpeg")
}

pub fn ffprobe_path() -> PathBuf {
    resolve("ffprobe")
}
