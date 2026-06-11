use std::path::PathBuf;

fn resolve(name: &str) -> PathBuf {
    if cfg!(debug_assertions) {
        // Dev builds run from target/, so reach into the repo's binaries dir.
        let triple = if cfg!(target_arch = "aarch64") {
            "aarch64-apple-darwin"
        } else {
            "x86_64-apple-darwin"
        };
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(format!("{name}-{triple}"))
    } else {
        // Tauri bundles externalBin next to the main binary, stripped of the triple.
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join(name)))
            .unwrap_or_else(|| PathBuf::from(name))
    }
}

pub fn ffmpeg_path() -> PathBuf {
    resolve("ffmpeg")
}

pub fn ffprobe_path() -> PathBuf {
    resolve("ffprobe")
}
