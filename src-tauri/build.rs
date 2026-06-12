fn main() {
    // The runtime needs the build target to find dev sidecars
    // (binaries/<name>-<triple><exe>); TARGET is only visible to build scripts.
    println!(
        "cargo:rustc-env=TAMP_TARGET_TRIPLE={}",
        std::env::var("TARGET").expect("cargo always sets TARGET")
    );
    tauri_build::build()
}
