# Windows Support — Design

**Date:** 2026-06-12
**Status:** Approved
**Goal:** tamp behaves on Windows the same way it behaves on macOS, with OS-specific
strategies isolated behind a compile-time platform hierarchy (Linux-ready), and a
GitHub Actions pipeline that builds and releases macOS + Windows from CI.

## Context

tamp is a Tauri 2 tray app: the frontend (vanilla TS) and the encoder engine
(bitrate planning, size-convergence retries, ffmpeg sidecar orchestration) are
already platform-neutral. The macOS-specific surface is small and enumerable:

| Area | Current macOS implementation |
| --- | --- |
| Clipboard file copy | NSPasteboard `writeObjects` (`platform/macos.rs`) |
| Panel window tweaks | `FullScreenAuxiliary` collection behavior; `set_visible_on_all_workspaces` in `lib.rs` |
| Reveal in file manager / open logs dir | `/usr/bin/open -R` / `/usr/bin/open` |
| Tray progress | `tray.set_title("42%")` — text next to the icon (macOS-only capability) |
| Free disk space logging | `libc::statvfs` |
| Hardware encoder | `h264_videotoolbox -allow_sw 1`, overshoot → two-pass x264 |
| Sidecar fetch | `scripts/fetch-ffmpeg.sh` (bash, martin-riedl.de macOS builds) |
| Sidecar path resolution | hardcoded `*-apple-darwin` triples in `encoder/bin.rs` (debug) |
| Default watched folder | `~/Desktop` |
| Bundling | `app` + `dmg` targets, `macOSPrivateApi` |
| CI/release | rust CI on `macos-14`; release builds aarch64 DMG only |

## Decisions (user-confirmed)

1. **Hardware encoding on Windows:** probe ffmpeg's available encoders at runtime
   and try the best candidate first — NVENC → QSV → AMF → Media Foundation —
   falling back to two-pass x264 via the existing convergence ladder.
2. **Tray progress on Windows:** dynamically rendered circular progress-ring tray
   icon + tooltip with the exact percentage (Windows tray cannot show title text).
3. **Installer:** NSIS, per-user (`%LOCALAPPDATA%`, no admin), unsigned —
   SmartScreen bypass documented in README (same posture as the unsigned DMG).
4. **Default watched folders on Windows:** `Desktop`, `Videos\Screen Recordings`
   (Snipping Tool), `Videos\Captures` (Xbox Game Bar) — those that exist;
   Desktop unconditionally.

## Architecture: compile-time platform hierarchy

Approach chosen over (a) per-function `#[cfg]` blocks — scatters OS conditionals,
and (b) runtime `dyn` strategy registry — needless indirection when the platform
is known at compile time.

`src-tauri/src/platform/` exposes a `Platform` trait with one implementation
struct per OS, selected at a **single cfg site**:

```rust
// platform/mod.rs (sketch)
pub trait Platform {
    fn copy_files_to_clipboard(&self, app: &AppHandle, paths: &[PathBuf]) -> Result<(), String>;
    fn configure_panel(&self, window: &WebviewWindow) -> Result<(), String>;
    fn default_watched_folders(&self, app: &AppHandle) -> Vec<PathBuf>;
    fn prepare_background_command(&self, cmd: &mut tokio::process::Command);
    fn tray_progress(&self, app: &AppHandle, fraction: Option<f64>);
    fn hw_candidates(&self) -> &[HwCandidate];
}

#[cfg(target_os = "macos")]   mod macos;
#[cfg(target_os = "windows")] mod windows;
#[cfg(target_os = "macos")]   pub fn native() -> &'static impl Platform { … MacOs … }
#[cfg(target_os = "windows")] pub fn native() -> &'static impl Platform { … Windows … }
```

No file outside `platform/` mentions an operating system. Adding Linux later =
`platform/linux.rs` + one selection line.

Per-OS behavior:

| Method | macOS | Windows |
| --- | --- | --- |
| `copy_files_to_clipboard` | NSPasteboard (existing code moves as-is) | `CF_HDROP` file list via `clipboard-win` |
| `configure_panel` | `FullScreenAuxiliary` + `set_visible_on_all_workspaces` (moves out of `lib.rs`) | no-op |
| `default_watched_folders` | `~/Desktop` | Desktop + `Videos\Screen Recordings` + `Videos\Captures` (existing dirs) |
| `prepare_background_command` | no-op | `CREATE_NO_WINDOW` creation flag (prevents console flash on every ffmpeg/ffprobe spawn) |
| `tray_progress` | `tray.set_title("42%")` (current behavior) | rasterized RGBA progress-ring icon (own pure module, unit-tested) + tooltip `tamp — 42%`; static icon restored on `None` |
| `hw_candidates` | `[h264_videotoolbox -allow_sw 1]` | probe `ffmpeg -encoders` once (cached): intersection of `[h264_nvenc, h264_qsv, h264_amf, h264_mf]` in that order |

### Platform code deleted via cross-platform replacements

- `commands::reveal` and `tray::open_logs_dir` (`/usr/bin/open`) →
  **`tauri-plugin-opener`** (`reveal_item_in_dir`, `open_path`) — supports
  macOS/Windows/Linux.
- `encoder::free_disk_bytes` (`libc::statvfs`, unsafe) → **`fs4::available_space`**.

## Encoder changes

The size-convergence policy (`encoder/retry.rs`) is **unchanged** — it already
models generic `Hardware`/`Software` kinds and its guarantee (never deliver an
over-target file) is platform-independent.

- New `encoder/hw.rs`: `HwCandidate { label, codec + extra ffmpeg args }`.
  `run_hardware_pass` is parametrized by candidate instead of hardcoding
  videotoolbox args.
- Worker hardware-attempt logic (platform-neutral): try candidates in order;
  **process error / missing output → next candidate**; **overshoot → two-pass
  x264** (overshoot means rate control lied — switching vendors won't fix it);
  candidates exhausted → software. On macOS (single candidate) this reduces to
  exactly today's behavior.
- WebM (two-pass VP9) and GIF engines are software-only and untouched.
- The `hardware_viable` bits-per-pixel gate stays as a generic pre-filter for
  all single-pass hardware encoders.

## Sidecars (ffmpeg/ffprobe)

- `scripts/fetch-ffmpeg.sh` → **`scripts/fetch-ffmpeg.ts`** (Bun, one script
  for all platforms): macOS → martin-riedl.de static builds (unchanged source);
  Windows → **BtbN FFmpeg-Builds** GPL static zips (`win64` / `winarm64`; both
  include libx264, libvpx, aac). Destination naming unchanged:
  `src-tauri/binaries/<bin>-<target-triple><exe-suffix>`.
- `encoder/bin.rs`: `build.rs` emits the real target triple
  (`cargo:rustc-env=TAMP_TARGET_TRIPLE`), paths use `std::env::consts::EXE_SUFFIX`
  — removes the hardcoded apple triples.

## Tauri config & bundling

- `tauri.conf.json` keeps the shared core; platform overlays (Tauri merges them
  natively): `tauri.macos.conf.json` — `app`+`dmg` targets, `macOSPrivateApi`,
  `minimumSystemVersion`, ad-hoc signing; **`tauri.windows.conf.json`** — `nsis`
  target, per-user install mode, WebView2 `downloadBootstrapper`.
- Asset-protocol scope additionally allows `$VIDEO/**` (runtime
  `allow_directory` already covers user-added watched folders).
- Existing `icon.ico` serves the Windows app and installer.

## Frontend

- New `src/lib/platform.ts`: queries a tiny new `os_info` command once at boot
  and exposes a strategy object — `revealLabel` ("Reveal in Finder" / "Show in
  Explorer"), accelerator display ("⌘⌥T" / "Ctrl+Alt+T"). Consumers (`list.ts`,
  `preferences.ts`) use the strategy; no OS conditionals outside this module.
- Stored accelerators already use `CmdOrCtrl` and need no migration.

## CI / Release pipeline

- **`ci.yml`** — rust job becomes a matrix: `macos-14` + `windows-latest`
  (rustfmt, clippy, unit + real-encode integration tests on both).
- **`prerelease.yml`** (new) — triggered by tag push `v*-beta*`. Tag-push
  workflows execute the workflow file **at the tagged commit**, so betas run
  from any branch with no default-branch change. Builds macOS DMG (aarch64),
  Windows NSIS x64 (`windows-latest`), Windows NSIS arm64 (`windows-11-arm`)
  and creates a GitHub **prerelease**. Beta versioning: `package.json` bumped
  manually on the branch (e.g. `0.3.0-beta.1`); changesets is not involved in
  betas.
- **`release.yml`** — the changesets version/tag flow is unchanged; the `build`
  job becomes the same 3-target matrix so stable releases ship macOS + Windows.
- All builds unsigned (as today); SmartScreen note in README.

## Behavior parity matrix (acceptance)

Tested on the Windows machine (Windows 11 ARM64, Parallels — hardware probe
lands on `h264_mf` there; NVENC/QSV/AMF ship guarded by candidate fallback):

1. Recording made with a default Windows tool (Snipping Tool / Game Bar)
   appears in the panel without configuration.
2. MP4 preset compresses under target via the hardware path; forced-software
   path also verified. WebM and GIF presets land under target.
3. Compressed file is on the clipboard as a *file* and pastes into a chat app.
4. Reveal-in-Explorer and Open Logs work.
5. Tray shows the progress ring + tooltip percentage during encodes.
6. Global shortcuts (Ctrl+Alt+T / Ctrl+Alt+O), autostart, move-original-to-
   Recycle-Bin, split-into-parts all work.
7. `cargo test` green on Windows; CI green on both OS runners.
8. Beta installed from the **pipeline-built** arm64 NSIS artifact re-passes the
   checklist.

## Docs

- README: Windows install section (NSIS, SmartScreen bypass), updated feature
  notes where macOS-specific.
- CONTRIBUTING: Windows prerequisites (rustup MSVC, VS Build Tools C++, Bun),
  cross-platform sidecar fetch.
- `docs/releasing.md`: beta-tag flow, stable release flow, and manual release
  steps as fallback.

## Risks

- **Vendor GPU encoders untested on real hardware** (VM has no passthrough) —
  guarded by candidate fallback + overshoot ladder; the ladder never delivers
  an over-target file regardless of encoder behavior.
- **`windows-11-arm` runner / BtbN `winarm64` asset naming** — verified at
  implementation; fallback: x64-only beta (runs emulated on ARM64 Windows).
- **Media Foundation rate-control accuracy unknown** — same ladder guarantee.
- **NSIS bundling on the arm64 runner** — makensis runs emulated x64 if needed.
