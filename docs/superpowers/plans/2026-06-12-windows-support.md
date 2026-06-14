# Windows Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** tamp runs on Windows with the same behavior as macOS (scan → compress under target → clipboard), with OS strategies isolated behind a compile-time platform hierarchy and a GitHub Actions pipeline that builds/releases macOS + Windows.

**Architecture:** A `Platform` trait in `src-tauri/src/platform/` with one impl per OS selected at a single `cfg` site; the encoder gains a platform-provided hardware-candidate list (probed against `ffmpeg -encoders`) while the size-convergence retry policy stays unchanged; macOS-only shims (`/usr/bin/open`, statvfs) are replaced by cross-platform crates (`tauri-plugin-opener`, `fs4`).

**Tech Stack:** Tauri 2 (Rust), vanilla TS + Vite (Bun), ffmpeg sidecars (martin-riedl.de for macOS, BtbN FFmpeg-Builds for Windows), GitHub Actions (`macos-14`, `windows-latest`, `windows-11-arm`).

**Spec:** `docs/superpowers/specs/2026-06-12-windows-support-design.md`

**Context for the worker:**
- This machine is **Windows 11 ARM64** (Parallels VM on Apple Silicon). Host rust triple: `aarch64-pc-windows-msvc`. The hardware-encoder probe lands on `h264_mf` here; NVENC/QSV/AMF cannot be exercised locally.
- Repo: `C:\Users\valeriimaslenykov\claude\tamp`, branch `windows-support`.
- Shell is PowerShell 5.1 — no `&&`; chain with `;`. `cargo` commands run from `src-tauri/`.
- Frontend tests: `bun run test` (vitest) from repo root. Rust: `cargo test` from `src-tauri/`.
- Commit after every task (messages given per task). Never commit `src-tauri/binaries/`.

---

### Task 1: Local Windows toolchain

**Files:** none (machine setup)

- [ ] **Step 1: Install rustup (ARM64 host)**

```powershell
winget install --id Rustlang.Rustup --accept-source-agreements --accept-package-agreements
```

If winget is unavailable, download https://win.rustup.rs/aarch64 and run it with defaults. Expected: `rustup` on PATH after reopening the shell (or add `$env:USERPROFILE\.cargo\bin` to PATH for this session).

- [ ] **Step 2: Install Visual Studio Build Tools with ARM64 C++ tools**

```powershell
winget install --id Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --add Microsoft.VisualStudio.Component.VC.Tools.ARM64 --add Microsoft.VisualStudio.Component.Windows11SDK.22621" --accept-source-agreements --accept-package-agreements
```

This may trigger a UAC prompt the user must confirm. Expected: install completes; `link.exe` exists under `C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\*\bin\Hostarm64\arm64\`.

- [ ] **Step 3: Verify the toolchain**

```powershell
rustup default stable; rustc --version; rustc -vV | Select-String host
```

Expected: `host: aarch64-pc-windows-msvc`.

- [ ] **Step 4: Verify the repo compiles (will fail only on missing sidecars, not toolchain)**

```powershell
bun install --frozen-lockfile; bun run build
cd src-tauri; cargo check; cd ..
```

Expected: `cargo check` succeeds (sidecars are runtime artifacts, not compile-time). If `tauri.conf.json`'s `externalBin` makes `tauri build` fail later without sidecars, that's expected until Task 2.

No commit (no repo changes).

---

### Task 2: Cross-platform sidecar fetcher (`fetch-ffmpeg.ts`)

**Files:**
- Create: `scripts/fetch-ffmpeg.ts`
- Delete: `scripts/fetch-ffmpeg.sh`

- [ ] **Step 1: Write the script**

Create `scripts/fetch-ffmpeg.ts`:

```ts
#!/usr/bin/env bun
// Downloads static ffmpeg/ffprobe builds and places them where Tauri expects
// sidecar binaries: src-tauri/binaries/<name>-<target-triple><exe-suffix>.
//
// macOS: GPL static builds from https://ffmpeg.martin-riedl.de
// Windows: GPL static builds from https://github.com/BtbN/FFmpeg-Builds
//   (asset names verified: ffmpeg-master-latest-win64-gpl.zip,
//    ffmpeg-master-latest-winarm64-gpl.zip — each contains bin/ffmpeg.exe
//    and bin/ffprobe.exe)
//
// Usage: bun scripts/fetch-ffmpeg.ts [arm64|x64]   (default: host arch)
import { existsSync } from "node:fs";
import { mkdir, rm, copyFile, chmod, mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";

const archArg = process.argv[2];
const arch = archArg ?? process.arch; // "arm64" | "x64"
if (arch !== "arm64" && arch !== "x64") {
  console.error(`Unsupported arch: ${arch} (expected arm64 or x64)`);
  process.exit(1);
}

const os = process.platform; // "darwin" | "win32"
const TRIPLES: Record<string, string> = {
  "darwin-arm64": "aarch64-apple-darwin",
  "darwin-x64": "x86_64-apple-darwin",
  "win32-arm64": "aarch64-pc-windows-msvc",
  "win32-x64": "x86_64-pc-windows-msvc",
};
const triple = TRIPLES[`${os}-${arch}`];
if (!triple) {
  console.error(`Unsupported platform: ${os}-${arch}`);
  process.exit(1);
}
const exe = os === "win32" ? ".exe" : "";
const destDir = join(import.meta.dir, "..", "src-tauri", "binaries");
await mkdir(destDir, { recursive: true });

async function download(url: string, to: string): Promise<void> {
  console.log(`↓ ${url}`);
  const res = await fetch(url, { redirect: "follow" });
  if (!res.ok) throw new Error(`HTTP ${res.status} for ${url}`);
  await Bun.write(to, res);
}

/** bsdtar ships with both macOS and Windows 10+ and extracts zips. */
async function extract(zip: string, into: string): Promise<void> {
  const p = Bun.spawn(["tar", "-xf", zip, "-C", into]);
  if ((await p.exited) !== 0) throw new Error(`tar failed on ${zip}`);
}

async function run(cmd: string[]): Promise<number> {
  const p = Bun.spawn(cmd, { stdout: "inherit", stderr: "inherit" });
  return await p.exited;
}

const dests = (["ffmpeg", "ffprobe"] as const).map((bin) => ({
  bin,
  dest: join(destDir, `${bin}-${triple}${exe}`),
}));
if (dests.every(({ dest }) => existsSync(dest))) {
  console.log("✓ sidecars already present, skipping (delete them to re-fetch)");
} else if (os === "darwin") {
  const riedlArch = arch === "arm64" ? "arm64" : "amd64";
  for (const { bin, dest } of dests) {
    if (existsSync(dest)) continue;
    const tmp = await mkdtemp(join(tmpdir(), "tamp-ffmpeg-"));
    const zip = join(tmp, `${bin}.zip`);
    await download(
      `https://ffmpeg.martin-riedl.de/redirect/latest/macos/${riedlArch}/release/${bin}.zip`,
      zip,
    );
    await extract(zip, tmp);
    await copyFile(join(tmp, bin), dest);
    await chmod(dest, 0o755);
    // Quarantine strip + ad-hoc sign keep Gatekeeper happy; best-effort.
    await run(["xattr", "-d", "com.apple.quarantine", dest]).catch(() => 1);
    if ((await run(["codesign", "-fs", "-", dest])) !== 0) {
      throw new Error(`codesign failed for ${dest}`);
    }
    await rm(tmp, { recursive: true, force: true });
    console.log(`✓ ${dest}`);
  }
} else {
  const btbn = arch === "arm64" ? "winarm64" : "win64";
  const name = `ffmpeg-master-latest-${btbn}-gpl`;
  const tmp = await mkdtemp(join(tmpdir(), "tamp-ffmpeg-"));
  const zip = join(tmp, `${name}.zip`);
  await download(
    `https://github.com/BtbN/FFmpeg-Builds/releases/latest/download/${name}.zip`,
    zip,
  );
  await extract(zip, tmp);
  for (const { bin, dest } of dests) {
    if (existsSync(dest)) continue;
    await copyFile(join(tmp, name, "bin", `${bin}${exe}`), dest);
    console.log(`✓ ${dest}`);
  }
  await rm(tmp, { recursive: true, force: true });
}

for (const { dest } of dests) {
  const p = Bun.spawn([dest, "-version"], { stdout: "pipe" });
  const firstLine = (await new Response(p.stdout).text()).split("\n")[0];
  console.log(firstLine);
}
```

- [ ] **Step 2: Run it on this machine**

```powershell
bun scripts/fetch-ffmpeg.ts
```

Expected output ends with two version lines, e.g. `ffmpeg version N-…` and `ffprobe version N-…`, and `src-tauri/binaries/ffmpeg-aarch64-pc-windows-msvc.exe` + `ffprobe-…exe` exist.

- [ ] **Step 3: Delete the bash script and update references**

```powershell
git rm scripts/fetch-ffmpeg.sh
```

Update the two `./scripts/fetch-ffmpeg.sh` references in `.github/workflows/*.yml` to `bun scripts/fetch-ffmpeg.ts` (full workflow rewrites land in Tasks 14–16, but keep CI green if it runs meanwhile), and the mentions in `README.md` (lines with `fetch-ffmpeg.sh`) and `CONTRIBUTING.md` to `bun scripts/fetch-ffmpeg.ts` (docs get fully rewritten in Task 17; a minimal find/replace is enough here).

- [ ] **Step 4: Commit**

```powershell
git add -A; git commit -m "build: cross-platform ffmpeg sidecar fetcher (macOS + Windows)"
```

---

### Task 3: Target-aware sidecar path resolution

**Files:**
- Modify: `src-tauri/build.rs`
- Modify: `src-tauri/src/encoder/bin.rs`

- [ ] **Step 1: Emit the target triple from build.rs**

Replace `src-tauri/build.rs` with:

```rust
fn main() {
    // The runtime needs the build target to find dev sidecars
    // (binaries/<name>-<triple><exe>); TARGET is only visible to build scripts.
    println!(
        "cargo:rustc-env=TAMP_TARGET_TRIPLE={}",
        std::env::var("TARGET").expect("cargo always sets TARGET")
    );
    tauri_build::build()
}
```

- [ ] **Step 2: Use it in bin.rs**

Replace the `resolve` function in `src-tauri/src/encoder/bin.rs` with:

```rust
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
```

- [ ] **Step 3: Verify**

```powershell
cd src-tauri; cargo check; cd ..
```

Expected: clean check.

- [ ] **Step 4: Commit**

```powershell
git add src-tauri/build.rs src-tauri/src/encoder/bin.rs
git commit -m "fix: resolve dev sidecars by build target instead of hardcoded apple triples"
```

---

### Task 4: Platform trait + per-OS modules (skeleton)

**Files:**
- Rewrite: `src-tauri/src/platform/mod.rs`
- Modify: `src-tauri/src/platform/macos.rs`
- Create: `src-tauri/src/platform/windows.rs`
- Modify: `src-tauri/src/lib.rs` (panel setup, callers)
- Modify: `src-tauri/src/commands.rs:314-323` (`copy_file` caller)
- Modify: `src-tauri/src/encoder/mod.rs:1193` (clipboard caller)

This task introduces the trait with macOS behavior preserved and Windows compiling with honest stubs; later tasks fill the Windows strategies one by one.

- [ ] **Step 1: Rewrite `platform/mod.rs`**

```rust
//! OS-specific strategies behind one interface. The ONLY place in the
//! codebase allowed to know which operating system it runs on is this
//! module's cfg-selected implementation; everything else calls
//! [`native()`] and stays platform-neutral. Adding an OS = one new module
//! implementing [`Platform`] plus one selection line below.

use std::path::PathBuf;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
static NATIVE: macos::MacOs = macos::MacOs;
#[cfg(target_os = "windows")]
static NATIVE: windows::Windows = windows::Windows;

/// The running OS's [`Platform`] strategy.
pub fn native() -> &'static impl Platform {
    &NATIVE
}

/// Live encode progress for the tray: how it's surfaced is per-OS (macOS can
/// render text next to the icon; Windows cannot).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrayProgress {
    /// Overall progress of the running job, 0..=1.
    pub fraction: f64,
    /// Jobs waiting behind it.
    pub queued: usize,
}

/// One hardware H.264 encoder this OS may offer; the encoder probes the
/// bundled ffmpeg for availability before use.
pub struct HwCandidate {
    /// ffmpeg encoder name as listed by `ffmpeg -encoders` (e.g.
    /// "h264_videotoolbox").
    pub name: &'static str,
    /// Extra codec args appended after `-c:v <name>` (rate args are shared).
    pub extra_args: &'static [&'static str],
}

pub trait Platform {
    /// Puts files (as file references, not contents) on the system clipboard
    /// in one write, so a multi-file paste lands all of them.
    fn copy_files_to_clipboard(
        &self,
        app: &tauri::AppHandle,
        paths: &[PathBuf],
    ) -> Result<(), String>;

    /// Per-OS window tweaks for the tray panel (e.g. on macOS, letting it
    /// appear over full-screen apps and follow the user across Spaces).
    fn configure_panel(&self, window: &tauri::WebviewWindow) -> Result<(), String>;

    /// Folders watched out of the box — wherever this OS's default screen
    /// recorders save.
    fn default_watched_folders(&self, app: &tauri::AppHandle) -> Vec<PathBuf>;

    /// Pre-spawn tweaks for background helper processes (ffmpeg/ffprobe);
    /// on Windows this suppresses the console window that would otherwise
    /// flash up for every spawn.
    fn prepare_background_command(&self, cmd: &mut tokio::process::Command);

    /// Surfaces encode progress on the tray; `None` clears it back to idle.
    fn tray_progress(&self, app: &tauri::AppHandle, progress: Option<TrayProgress>);

    /// Hardware H.264 encoders this OS can offer, in preference order.
    /// Empty means hardware encoding is never attempted.
    fn hw_candidates(&self) -> &'static [HwCandidate];
}

/// Single-file convenience over [`Platform::copy_files_to_clipboard`].
pub fn copy_file_to_clipboard(app: &tauri::AppHandle, path: &std::path::Path) -> Result<(), String> {
    native().copy_files_to_clipboard(app, &[path.to_path_buf()])
}

/// A `tokio::process::Command` pre-configured for background helpers.
pub fn background_command(program: impl AsRef<std::ffi::OsStr>) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(program);
    native().prepare_background_command(&mut cmd);
    cmd
}
```

- [ ] **Step 2: Convert `platform/macos.rs` to the trait**

Keep the two existing functions' bodies exactly as they are (NSPasteboard write, FullScreenAuxiliary), but restructure the file: add at the top

```rust
use super::{HwCandidate, Platform, TrayProgress};
```

declare the strategy struct and move the functions into the impl (bodies unchanged — `configure_panel` additionally absorbs the `set_visible_on_all_workspaces` call that Task's Step 4 removes from `lib.rs`):

```rust
pub struct MacOs;

impl Platform for MacOs {
    fn copy_files_to_clipboard(
        &self,
        app: &tauri::AppHandle,
        paths: &[PathBuf],
    ) -> Result<(), String> {
        copy_files_to_clipboard(app, paths)
    }

    fn configure_panel(&self, window: &tauri::WebviewWindow) -> Result<(), String> {
        // Tray panels must follow the user across Spaces/displays; without
        // this the panel opens on the Space the app launched on.
        window
            .set_visible_on_all_workspaces(true)
            .map_err(|e| format!("failed to set panel visible on all workspaces: {e}"))?;
        configure_panel(window)
    }

    fn default_watched_folders(&self, app: &tauri::AppHandle) -> Vec<PathBuf> {
        // ⌘⇧5 saves to the Desktop by default.
        match tauri::Manager::path(app).desktop_dir() {
            Ok(desktop) => vec![desktop],
            Err(e) => {
                crate::log_warn!("cannot resolve desktop dir for default watched folder: {e}");
                Vec::new()
            }
        }
    }

    fn prepare_background_command(&self, _cmd: &mut tokio::process::Command) {}

    fn tray_progress(&self, app: &tauri::AppHandle, progress: Option<TrayProgress>) {
        // Tray title text next to the icon is a macOS-only capability.
        let text = progress.map(|p| {
            let pct = (p.fraction.clamp(0.0, 1.0) * 100.0).round() as u32;
            if p.queued > 0 {
                format!("{pct}% (+{})", p.queued)
            } else {
                format!("{pct}%")
            }
        });
        crate::tray::set_title(app, text);
    }

    fn hw_candidates(&self) -> &'static [HwCandidate] {
        // `-allow_sw 1` lets VideoToolbox use Apple's software encoder when
        // no hardware session is available.
        &[HwCandidate {
            name: "h264_videotoolbox",
            extra_args: &["-allow_sw", "1"],
        }]
    }
}
```

(`crate::tray::set_title` is introduced in Step 5. The existing free functions `configure_panel` / `copy_files_to_clipboard` become private helpers in the same file.)

- [ ] **Step 3: Create `platform/windows.rs` with honest stubs**

```rust
use std::path::PathBuf;

use super::{HwCandidate, Platform, TrayProgress};

pub struct Windows;

impl Platform for Windows {
    fn copy_files_to_clipboard(
        &self,
        _app: &tauri::AppHandle,
        _paths: &[PathBuf],
    ) -> Result<(), String> {
        Err("clipboard file copy is not implemented on Windows yet".to_string())
    }

    fn configure_panel(&self, _window: &tauri::WebviewWindow) -> Result<(), String> {
        // No Spaces/full-screen-auxiliary equivalents to configure.
        Ok(())
    }

    fn default_watched_folders(&self, app: &tauri::AppHandle) -> Vec<PathBuf> {
        match tauri::Manager::path(app).desktop_dir() {
            Ok(desktop) => vec![desktop],
            Err(e) => {
                crate::log_warn!("cannot resolve desktop dir for default watched folder: {e}");
                Vec::new()
            }
        }
    }

    fn prepare_background_command(&self, _cmd: &mut tokio::process::Command) {}

    fn tray_progress(&self, _app: &tauri::AppHandle, _progress: Option<TrayProgress>) {}

    fn hw_candidates(&self) -> &'static [HwCandidate] {
        &[]
    }
}
```

(Each stub is replaced with the real strategy in Tasks 6, 7, 9, 10, 11.)

- [ ] **Step 4: Update the callers**

In `src-tauri/src/lib.rs` (setup hook, lines 190-199): replace the panel block with

```rust
            if let Some(panel) = app.get_webview_window("panel") {
                if let Err(e) = platform::native().configure_panel(&panel) {
                    log_warn!("failed to configure panel: {e}");
                }
            }
```

and add `use platform::Platform as _;` next to the existing `use` items.

In `src-tauri/src/commands.rs` `copy_file` (line 319): `crate::platform::copy_file_to_clipboard(&app, Path::new(&path))` — unchanged call, still compiles via the new free function.

In `src-tauri/src/encoder/mod.rs` `run_post_actions` (line 1193): replace `crate::platform::copy_files_to_clipboard(&inner.app, outputs)` with `crate::platform::native().copy_files_to_clipboard(&inner.app, outputs)` and add `use crate::platform::Platform as _;` to the file's imports.

- [ ] **Step 5: Split tray title out of `set_progress`**

In `src-tauri/src/tray.rs`, replace `pub fn set_progress` (lines 85-92) with:

```rust
/// Sets the text shown next to the tray icon (macOS-only capability; the
/// macOS platform strategy is the only caller).
pub fn set_title(app: &AppHandle, text: Option<String>) {
    let Some(tray) = app.tray_by_id("main") else {
        return;
    };
    if let Err(e) = tray.set_title(text.as_deref()) {
        crate::log_warn!("failed to set tray title: {e}");
    }
}
```

In `src-tauri/src/encoder/mod.rs`, replace `update_tray` (lines 278-289) with:

```rust
fn update_tray(inner: &Inner, progress: Option<f64>) {
    let progress = progress.map(|p| crate::platform::TrayProgress {
        fraction: p,
        queued: inner.pending.load(Ordering::SeqCst),
    });
    crate::platform::native().tray_progress(&inner.app, progress);
}
```

and replace the two `crate::tray::set_progress(&inner.app, None);` calls (lines 298 and 349) with `update_tray(inner, None);`.

On non-macOS, `set_title` has no caller — gate it to keep clippy's dead-code lint green:

```rust
#[cfg(target_os = "macos")]
pub fn set_title(app: &AppHandle, text: Option<String>) {
```

(This is a capability shim next to the tray it controls, not strategy logic — strategies stay in `platform/`.)

- [ ] **Step 6: Verify on Windows, then commit**

```powershell
cd src-tauri; cargo check; cargo clippy --all-targets -- -D warnings; cargo test; cd ..
```

Expected: all green (clipboard tests don't exist; integration tests run software paths; the `hardware_single_pass…` integration test still compiles because `run_hardware_pass` is untouched so far).

```powershell
git add -A; git commit -m "refactor: platform trait hierarchy with per-OS strategy modules"
```

---

### Task 5: Cross-platform reveal/open via tauri-plugin-opener

**Files:**
- Modify: `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`, `src-tauri/src/commands.rs:325-340`, `src-tauri/src/tray.rs:39-60`
- Create: `src/lib/platform.ts`
- Modify: `src/main.ts`, `src/views/list.ts:626-642`

- [ ] **Step 1: Add the plugin**

`src-tauri/Cargo.toml` dependencies: add `tauri-plugin-opener = "2"`. Then `cd src-tauri; cargo fetch`.

`src-tauri/src/lib.rs`: add `.plugin(tauri_plugin_opener::init())` after the other `.plugin(...)` lines.

- [ ] **Step 2: Replace `reveal` and add `os_info`**

In `src-tauri/src/commands.rs`, replace the whole `reveal` command (lines 325-340) with:

```rust
#[tauri::command]
pub fn reveal(app: AppHandle, path: String) {
    use tauri_plugin_opener::OpenerExt as _;
    if let Err(e) = app.opener().reveal_item_in_dir(&path) {
        crate::log_error!("failed to reveal {path}: {e}");
    }
}

/// The backend OS ("macos" | "windows" | "linux") for per-platform UI labels.
#[tauri::command]
pub fn os_info() -> &'static str {
    std::env::consts::OS
}
```

Register `commands::os_info` in the `tauri::generate_handler![...]` list in `lib.rs`.

- [ ] **Step 3: Replace `open_logs_dir`'s `/usr/bin/open`**

In `src-tauri/src/tray.rs`, the doc comment becomes "Opens the app's log directory in the system file manager." and the `#[cfg(target_os = "macos")] … Command::new("/usr/bin/open")` block (lines 53-59) becomes:

```rust
    use tauri_plugin_opener::OpenerExt as _;
    if let Err(e) = app.opener().open_path(dir.to_string_lossy(), None::<&str>) {
        crate::log_error!("failed to open log dir {}: {e}", dir.display());
    }
```

- [ ] **Step 4: Frontend platform module**

Create `src/lib/platform.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";

type OS = "macos" | "windows" | "linux";

let os: OS = "macos";

/** Resolves the backend OS once at boot; UI strings fall back to macOS. */
export async function initPlatform(): Promise<void> {
  try {
    os = (await invoke<string>("os_info")) as OS;
  } catch {
    /* keep the default */
  }
}

/**
 * The only place the frontend is allowed to branch on the OS: user-visible
 * strings naming OS concepts (file manager, etc.).
 */
export function revealLabel(): string {
  return os === "macos" ? "Reveal in Finder" : "Show in Explorer";
}
```

In `src/main.ts`: `import { initPlatform } from "./lib/platform";` and make the boot IIFE start with `await initPlatform();` (before `getSettings()`).

In `src/views/list.ts`: `import { revealLabel } from "../lib/platform";`; in `buildRevealButton` replace `btn.title = "Reveal in Finder";` with `btn.title = revealLabel();` and reword the two "in Finder" comments (lines 626 and 295 in `styles.css` need no change — CSS comment only; update it anyway to "system file manager" for accuracy).

- [ ] **Step 5: Verify and commit**

```powershell
bun run test; bun run build
cd src-tauri; cargo check; cargo clippy --all-targets -- -D warnings; cd ..
```

Expected: green. (`reveal_item_in_dir`/`open_path` are Rust-side calls — no capability entries needed.)

```powershell
git add -A; git commit -m "feat: cross-platform reveal/open via opener plugin, per-OS UI labels"
```

---

### Task 6: Windows clipboard file copy (CF_HDROP)

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/platform/windows.rs`

- [ ] **Step 1: Add the dependency (Windows-only)**

In `src-tauri/Cargo.toml`:

```toml
[target.'cfg(target_os = "windows")'.dependencies]
clipboard-win = "5"
```

- [ ] **Step 2: Implement**

Replace the `copy_files_to_clipboard` stub in `platform/windows.rs` with:

```rust
    /// Writes ALL `paths` as a CF_HDROP file list in one clipboard write, so
    /// pasting into Explorer/Discord/Slack drops the whole set at once.
    fn copy_files_to_clipboard(
        &self,
        _app: &tauri::AppHandle,
        paths: &[PathBuf],
    ) -> Result<(), String> {
        if paths.is_empty() {
            return Err("no files to copy".to_string());
        }
        let path_strs = paths
            .iter()
            .map(|p| {
                p.to_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("path is not valid UTF-8: {}", p.display()))
            })
            .collect::<Result<Vec<String>, String>>()?;
        let _clip = clipboard_win::Clipboard::new_attempts(10)
            .map_err(|e| format!("cannot open clipboard: {e}"))?;
        clipboard_win::raw::set_file_list(&path_strs)
            .map_err(|e| format!("clipboard file-list write failed: {e}"))
    }
```

If `clipboard_win`'s API differs at compile time (it has changed between majors), check `cargo doc -p clipboard-win --no-deps` — the requirement is: open clipboard with retries, write a CF_HDROP file list, no other formats.

- [ ] **Step 3: Verify and commit**

```powershell
cd src-tauri; cargo check; cargo clippy --all-targets -- -D warnings; cd ..
git add -A; git commit -m "feat(windows): clipboard file copy via CF_HDROP"
```

(Functional verification happens in Task 13's manual checklist: paste into a chat app.)

---

### Task 7: Windows tray progress ring

**Files:**
- Create: `src-tauri/src/platform/windows_ring.rs` (pure pixel math, unit-tested)
- Modify: `src-tauri/src/platform/windows.rs`, `src-tauri/src/platform/mod.rs`

- [ ] **Step 1: Write the failing test + renderer module**

Create `src-tauri/src/platform/windows_ring.rs`:

```rust
//! Rasterizes the tray progress ring: a clockwise-from-12-o'clock arc on a
//! faint full circle, white (tray icons sit on the dark taskbar). Pure pixel
//! math — no OS calls — so it's unit-testable everywhere.

/// Returns `size`×`size` RGBA pixels for `fraction` (0..=1) progress.
pub fn render(fraction: f64, size: u32) -> Vec<u8> {
    let fraction = fraction.clamp(0.0, 1.0);
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let center = (size as f64 - 1.0) / 2.0;
    let outer = size as f64 / 2.0 - 1.0;
    let inner = outer * 0.62;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f64 - center;
            let dy = y as f64 - center;
            let r = (dx * dx + dy * dy).sqrt();
            if r < inner || r > outer {
                continue;
            }
            // Angle clockwise from 12 o'clock, normalized to 0..1.
            let turn = (dx.atan2(-dy) / std::f64::consts::TAU).rem_euclid(1.0);
            let alpha: u8 = if turn <= fraction { 255 } else { 60 };
            let i = ((y * size + x) * 4) as usize;
            rgba[i..i + 3].copy_from_slice(&[255, 255, 255]);
            rgba[i + 3] = alpha;
        }
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::render;

    const SIZE: u32 = 32;

    fn alpha_at(rgba: &[u8], x: u32, y: u32) -> u8 {
        rgba[((y * SIZE + x) * 4 + 3) as usize]
    }

    #[test]
    fn center_is_transparent_ring_is_opaque_when_done() {
        let px = render(1.0, SIZE);
        assert_eq!(alpha_at(&px, 16, 16), 0, "center must stay empty");
        assert_eq!(alpha_at(&px, 16, 2), 255, "12 o'clock on the ring band");
    }

    #[test]
    fn zero_progress_leaves_only_the_faint_track() {
        let px = render(0.0, SIZE);
        assert!(px.chunks(4).all(|p| p[3] == 0 || p[3] == 60));
    }

    #[test]
    fn half_progress_fills_right_side_only() {
        let px = render(0.5, SIZE);
        assert_eq!(alpha_at(&px, 28, 16), 255, "3 o'clock filled");
        assert_eq!(alpha_at(&px, 3, 16), 60, "9 o'clock still faint");
    }

    #[test]
    fn out_of_range_fractions_clamp() {
        assert_eq!(render(-1.0, SIZE), render(0.0, SIZE));
        assert_eq!(render(2.0, SIZE), render(1.0, SIZE));
    }
}
```

Register it in `platform/mod.rs` (test-visible on every OS so CI exercises the math on macOS too):

```rust
#[cfg(any(target_os = "windows", test))]
mod windows_ring;
```

- [ ] **Step 2: Run the tests**

```powershell
cd src-tauri; cargo test windows_ring; cd ..
```

Expected: 4 passed.

- [ ] **Step 3: Wire it into the Windows strategy**

In `platform/windows.rs`, replace the `tray_progress` stub with:

```rust
    /// Windows tray icons can't carry text, so progress is a rendered ring
    /// icon plus the exact percentage in the tooltip.
    fn tray_progress(&self, app: &tauri::AppHandle, progress: Option<TrayProgress>) {
        let Some(tray) = app.tray_by_id("main") else {
            return;
        };
        let result = match progress {
            Some(p) => {
                let pct = (p.fraction.clamp(0.0, 1.0) * 100.0).round() as u32;
                let tooltip = if p.queued > 0 {
                    format!("tamp — {pct}% (+{} queued)", p.queued)
                } else {
                    format!("tamp — {pct}%")
                };
                const SIZE: u32 = 32;
                let icon =
                    tauri::image::Image::new_owned(super::windows_ring::render(p.fraction, SIZE), SIZE, SIZE);
                tray.set_icon(Some(icon)).and_then(|()| tray.set_tooltip(Some(&tooltip)))
            }
            None => tray
                .set_icon(Some(tauri::include_image!("icons/trayicon.png")))
                .and_then(|()| tray.set_tooltip(Some("tamp"))),
        };
        if let Err(e) = result {
            crate::log_warn!("failed to update tray progress: {e}");
        }
    }
```

Add `use tauri::Manager as _;` to the file's imports if `tray_by_id` needs it.

- [ ] **Step 4: Verify and commit**

```powershell
cd src-tauri; cargo test; cargo clippy --all-targets -- -D warnings; cd ..
git add -A; git commit -m "feat(windows): tray progress ring icon + tooltip percentage"
```

---

### Task 8: Cross-platform free-disk-space (delete unsafe statvfs)

**Files:**
- Modify: `src-tauri/Cargo.toml`, `src-tauri/src/encoder/mod.rs:1648-1668`

- [ ] **Step 1: Swap the implementation**

`Cargo.toml`: add `fs4 = "0.13"` to `[dependencies]`; remove `libc` from the macOS-only dependency block (it was only there for statvfs — check `rg 'libc' src-tauri/src` first; if other uses exist, keep it).

Replace both `free_disk_bytes` variants (`#[cfg(target_os = "macos")]` and `#[cfg(not(...))]`, lines 1648-1668 in `encoder/mod.rs`) with one:

```rust
/// Free bytes on the volume holding `path` — logged when an encode attempt
/// fails, since a full output volume is the classic transient cause that a
/// re-run hours later doesn't reproduce.
fn free_disk_bytes(path: &Path) -> Option<u64> {
    fs4::available_space(path).ok()
}
```

(If `fs4`'s free-function API moved, the same function exists as `fs4::available_space` in 0.x — check `cargo doc -p fs4 --no-deps`.)

- [ ] **Step 2: Verify and commit**

```powershell
cd src-tauri; cargo test; cargo clippy --all-targets -- -D warnings; cd ..
git add -A; git commit -m "refactor: cross-platform free-disk-space via fs4, drop unsafe statvfs"
```

---

### Task 9: Per-platform default watched folders

**Files:**
- Modify: `src-tauri/src/platform/windows.rs`, `src-tauri/src/settings.rs:160-170`

- [ ] **Step 1: Windows strategy**

Replace the `default_watched_folders` stub in `platform/windows.rs` with:

```rust
    /// Desktop plus wherever the stock Windows recorders save: Snipping Tool
    /// → Videos\Screen Recordings, Xbox Game Bar → Videos\Captures. The
    /// Videos subfolders are only watched when they exist (they appear after
    /// first use of the respective tool); Desktop is watched unconditionally.
    fn default_watched_folders(&self, app: &tauri::AppHandle) -> Vec<PathBuf> {
        let path = tauri::Manager::path(app);
        let mut folders = Vec::new();
        match path.desktop_dir() {
            Ok(desktop) => folders.push(desktop),
            Err(e) => crate::log_warn!("cannot resolve desktop dir for default watched folder: {e}"),
        }
        match path.video_dir() {
            Ok(videos) => folders.extend(
                ["Screen Recordings", "Captures"]
                    .iter()
                    .map(|sub| videos.join(sub))
                    .filter(|dir| dir.is_dir()),
            ),
            Err(e) => crate::log_warn!("cannot resolve videos dir for default watched folders: {e}"),
        }
        folders
    }
```

- [ ] **Step 2: Use the strategy in settings**

In `src-tauri/src/settings.rs` `default_settings` (lines 160-170), replace the `watched_folders` computation with:

```rust
    use crate::platform::Platform as _;
    let watched_folders = crate::platform::native()
        .default_watched_folders(app)
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
```

and update the doc comment to: "Default settings. Needs the app handle because the default watched folders come from the platform strategy (wherever this OS's screen recorders save)."

- [ ] **Step 3: Verify and commit**

```powershell
cd src-tauri; cargo test; cd ..
git add -A; git commit -m "feat(windows): watch Desktop + Videos recording folders by default"
```

---

### Task 10: No console flash — route every helper spawn through `background_command`

**Files:**
- Modify: `src-tauri/src/platform/windows.rs`
- Modify spawn sites: `src-tauri/src/encoder/mod.rs:1318,1492,1535`, `src-tauri/src/encoder/probe.rs:13,76`, `src-tauri/src/thumbs.rs:41`, `src-tauri/src/previews.rs:139,188`

- [ ] **Step 1: Implement the Windows hook**

Replace the `prepare_background_command` stub in `platform/windows.rs` with:

```rust
    /// tamp is a windows-subsystem app; without CREATE_NO_WINDOW every
    /// ffmpeg/ffprobe spawn flashes a console window over the user's screen.
    fn prepare_background_command(&self, cmd: &mut tokio::process::Command) {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
```

(`creation_flags` exists on `tokio::process::Command` under Windows; no extra import needed.)

- [ ] **Step 2: Replace all 8 helper spawn sites**

Each `tokio::process::Command::new(X)` where X is an ffmpeg/ffprobe path becomes `crate::platform::background_command(X)` (in `encoder/`, `super::bin::…` paths keep their expressions):

- `encoder/mod.rs:1318` → `let mut cmd = crate::platform::background_command(bin::ffmpeg_path());`
- `encoder/mod.rs:1492` → same
- `encoder/mod.rs:1535` → same
- `encoder/probe.rs:13` and `:76` → `crate::platform::background_command(super::bin::ffprobe_path())`
- `thumbs.rs:41` → `crate::platform::background_command(ffmpeg)`
- `previews.rs:139` and `:188` → `crate::platform::background_command(&ffmpeg)`

Verify none remain:

```powershell
Select-String -Path src-tauri\src -Pattern "process::Command::new" -Recurse
```

Expected: no matches in ffmpeg/ffprobe paths (platform/macos.rs has none; tray.rs's `/usr/bin/open` was removed in Task 5).

- [ ] **Step 3: Verify and commit**

```powershell
cd src-tauri; cargo test; cargo clippy --all-targets -- -D warnings; cd ..
git add -A; git commit -m "fix(windows): suppress console window for every ffmpeg/ffprobe spawn"
```

---

### Task 11: Hardware encoder candidates (probe + ladder integration)

**Files:**
- Create: `src-tauri/src/encoder/hw.rs`
- Modify: `src-tauri/src/encoder/mod.rs` (module decl; `run_hardware_pass` lines 1305-1360; `convergence_attempts` lines 1098-1178; the two `hardware_viable` call sites' log strings at 507-514 and 788-795)
- Modify: `src-tauri/src/platform/windows.rs` (`hw_candidates`)
- Modify: `src-tauri/tests/encode_integration.rs:225-270`

- [ ] **Step 1: Windows candidate list**

In `platform/windows.rs`, replace the `hw_candidates` stub with:

```rust
    fn hw_candidates(&self) -> &'static [HwCandidate] {
        // Vendor order: dedicated encoders first, Media Foundation (always
        // present, GPU MFT when there is one, software MFT otherwise) last.
        // Availability is probed against the bundled ffmpeg; a candidate
        // that fails at runtime falls through to the next, and overshoot
        // switches to two-pass x264 via the retry ladder.
        &[
            HwCandidate { name: "h264_nvenc", extra_args: &[] },
            HwCandidate { name: "h264_qsv", extra_args: &[] },
            HwCandidate { name: "h264_amf", extra_args: &[] },
            HwCandidate { name: "h264_mf", extra_args: &[] },
        ]
    }
```

- [ ] **Step 2: Probe module**

Create `src-tauri/src/encoder/hw.rs`:

```rust
//! Which hardware H.264 encoder to try: the platform names its candidates
//! in preference order; this module filters them against what the bundled
//! ffmpeg actually ships (`-encoders`), once per process.

use crate::platform::{HwCandidate, Platform as _};
use tokio::sync::OnceCell;

static AVAILABLE: OnceCell<Vec<&'static HwCandidate>> = OnceCell::const_new();

/// The platform's hardware candidates that the bundled ffmpeg supports, in
/// preference order. Probed once and cached; an empty slice means every
/// MP4 encode goes straight to two-pass software.
pub async fn available_candidates() -> &'static [&'static HwCandidate] {
    AVAILABLE
        .get_or_init(|| async {
            let names = match encoder_list().await {
                Ok(names) => names,
                Err(e) => {
                    crate::log_warn!("cannot probe ffmpeg encoders ({e}); hardware encoding disabled");
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
                available.iter().map(|c| c.name).collect::<Vec<_>>().join(", ")
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
```

Add `pub mod hw;` next to the other module declarations at the top of `encoder/mod.rs`.

(`names.contains(c.name)` is a substring match on the `-encoders` table — encoder names like `h264_mf` don't collide with other table text.)

- [ ] **Step 3: Parametrize `run_hardware_pass`**

Change its signature (line 1310) to take the candidate, and replace the hardcoded codec args (lines 1335-1338):

```rust
pub async fn run_hardware_pass(
    cand: &crate::platform::HwCandidate,
    plan: &plan::EncodePlan,
    info: &probe::ProbeInfo,
    input: &Path,
    child_slot: &ChildSlot,
    is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    on_progress: &mut (dyn FnMut(u8, f64) + Send),
) -> Result<(), String> {
```

```rust
    cmd.args(["-c:v", cand.name])
        .arg("-b:v")
        .arg(format!("{}k", plan.video_kbit))
        .args(cand.extra_args);
```

Update its doc comment to: "Runs the single-pass hardware encode for `plan` on `cand`, reporting progress as pass 1 mapped to the FULL 0..1 range. The worker falls through to the next candidate (and ultimately two-pass software) when this errors or leaves a missing/empty output. Public for the integration test."

- [ ] **Step 4: Candidate fall-through in `convergence_attempts`**

Replace the hardware block (lines 1109-1126) with:

```rust
    let mut candidates: std::collections::VecDeque<&'static crate::platform::HwCandidate> =
        if use_hardware && plan.format == OutputFormat::Mp4 {
            hw::available_candidates().await.iter().copied().collect()
        } else {
            Default::default()
        };
    for attempt in 0..=retry::MAX_RE_ENCODES {
        // Hardware attempt: first candidate that produces output wins; a
        // candidate that errors or writes nothing falls through to the next
        // (same attempt — no convergence info was gained), and an empty
        // queue means two-pass software.
        let mut ran_hardware = false;
        while let Some(cand) = candidates.front().copied() {
            let hw = run_hardware_pass(cand, plan, info, input, child_slot, is_cancelled, on_progress)
                .await;
            let failure = match hw {
                Ok(()) => match std::fs::metadata(&plan.output) {
                    Ok(meta) if meta.len() > 0 => None,
                    _ => Some(format!("{} produced no output", cand.name)),
                },
                Err(e) if is_cancelled() => return Err(e),
                Err(e) => Some(e),
            };
            match failure {
                None => {
                    ran_hardware = true;
                    break;
                }
                Some(reason) => {
                    crate::log_warn!("{reason}; trying the next encoder");
                    candidates.pop_front();
                }
            }
        }
        if !ran_hardware {
            // run_passes restarts progress from 0 on its own first callback.
            run_passes(plan, info, input, passlog_dir, child_slot, is_cancelled, on_progress)
                .await?;
        }
```

and below, where the attempt is judged (lines 1144-1148 and 1166):

```rust
        let encoder = if ran_hardware {
            retry::EncoderKind::Hardware
        } else {
            retry::EncoderKind::Software
        };
```

In the `Retry` arm, replace `use_hardware = next_encoder == retry::EncoderKind::Hardware;` with:

```rust
                if next_encoder == retry::EncoderKind::Software {
                    // Overshoot means rate control lied — switching hardware
                    // vendors won't fix it; all retries run two-pass x264.
                    candidates.clear();
                }
```

Delete the now-unused first line `let mut use_hardware = use_hardware && plan.format == OutputFormat::Mp4;` (the candidates initialization above absorbs it). The function's `use_hardware: bool` parameter stays.

Also update the two `hardware_viable` log strings (lines 509 and 790) — they name VideoToolbox; make them encoder-neutral: replace `under VideoToolbox's ~{HW_MIN_BPP} quality floor — it would ignore the rate and overshoot` with `under the ~{HW_MIN_BPP} single-pass hardware quality floor — it would ignore the rate and overshoot` (and the same for line 790). Update `HW_MIN_BPP`'s doc comment first sentence to "Bits per pixel per frame below which single-pass hardware encoders' quality floors IGNORE the requested bitrate (h264_videotoolbox once emitted 89.7 MB for a 498 kbit request):".

- [ ] **Step 5: Adapt the integration test**

In `src-tauri/tests/encode_integration.rs` (the `hardware_single_pass_spans_full_progress_range` test, lines 225-270): it currently checks `h264_videotoolbox` is in the ffmpeg build and calls `run_hardware_pass(...)` without a candidate. Make it platform-neutral:

```rust
use tamp_lib::platform::{self, Platform as _};
```

then at the top of the test, replace the videotoolbox availability check with:

```rust
    let Some(cand) = platform::native().hw_candidates().first() else {
        eprintln!("skipping hardware encode test: no hardware candidates on this platform");
        return;
    };
    // (keep the existing `-encoders` output check, but grep for cand.name
    // instead of the literal "h264_videotoolbox")
```

and pass `cand` as the new first argument of `run_hardware_pass(cand, …)`. The skip message at line 267 becomes `eprintln!("skipping hardware encode test: {} unavailable here: {e}", cand.name);`.

- [ ] **Step 6: Verify (this machine exercises h264_mf!) and commit**

```powershell
cd src-tauri; cargo test; cargo clippy --all-targets -- -D warnings; cd ..
```

Expected: all tests pass; the integration test's hardware case runs `h264_mf` here (or skips with a clear message if the BtbN build lacks it — then the convergence path is still covered by the software tests).

```powershell
git add -A; git commit -m "feat: per-platform hardware encoder candidates with probe and fall-through"
```

---

### Task 12: Tauri config split (macOS / Windows overlays)

**Files:**
- Modify: `src-tauri/tauri.conf.json`
- Create: `src-tauri/tauri.macos.conf.json`, `src-tauri/tauri.windows.conf.json`

- [ ] **Step 1: Split the config**

`src-tauri/tauri.conf.json` — remove `app.macOSPrivateApi`, `bundle.targets`, and `bundle.macOS`; widen the asset scope:

```json
    "assetProtocol": {
      "enable": true,
      "scope": ["$HOME/Desktop/**", "$VIDEO/**", "$APPCACHE/**"]
    }
```

Create `src-tauri/tauri.macos.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "app": {
    "macOSPrivateApi": true
  },
  "bundle": {
    "targets": ["app", "dmg"],
    "macOS": {
      "minimumSystemVersion": "12.0",
      "signingIdentity": "-"
    }
  }
}
```

Create `src-tauri/tauri.windows.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "bundle": {
    "targets": ["nsis"],
    "windows": {
      "webviewInstallMode": { "type": "downloadBootstrapper" },
      "nsis": { "installMode": "currentUser" }
    }
  }
}
```

(Tauri merges `tauri.<platform>.conf.json` over the base automatically.)

- [ ] **Step 2: Verify with a real Windows bundle**

```powershell
bun tauri build
```

Expected: `src-tauri/target/release/bundle/nsis/tamp_0.2.0_arm64-setup.exe` exists. If the version renders differently in the filename, note the actual pattern — Tasks 15/16 glob `*-setup.exe` so exact naming doesn't matter.

- [ ] **Step 3: Commit**

```powershell
git add -A; git commit -m "build: per-platform tauri config overlays (dmg vs nsis)"
```

---

### Task 13: Full local verification on this Windows machine

**Files:** none (verification gate; fix-forward anything found)

- [ ] **Step 1: Automated checks**

```powershell
bun run test; bun run build
cd src-tauri; cargo fmt; cargo clippy --all-targets -- -D warnings; cargo test; cd ..
```

Expected: everything green. If a Rust test fails on Windows-specific behavior (path separators, file locking on open handles, trash), fix the code so behavior matches macOS semantics — these are product bugs, not test bugs (exception: a test asserting a macOS-only constant). Re-run until green.

- [ ] **Step 2: Manual behavior-parity checklist (`bun tauri dev`)**

Run `bun tauri dev` and verify against the spec's acceptance matrix — user participation needed for the recording step:

1. Tray icon appears; left-click toggles the panel under the tray area; right-click shows Open Logs / Quit.
2. Record a short screen video with a default Windows tool (Snipping Tool Win+Shift+R, or ask the user); confirm it appears in the panel without adding folders.
3. Click the row → MP4 compresses under 10 MB; tray shows the ring + tooltip %.
4. Forced software path: temporarily toggle "hardware encoder" off in Preferences → encode again → still under target.
5. WebM preset and GIF custom conversion land under target.
6. Clipboard: paste the result as a file into a chat app/Explorer.
7. Show in Explorer reveals the file; Open Logs opens the log folder.
8. Ctrl+Alt+T compresses the latest recording; Ctrl+Alt+O toggles the panel.
9. "Move original to Trash" puts the original in the Recycle Bin.
10. Split-into-parts produces N parts, all on the clipboard.
11. Quit + relaunch: settings persisted; launch-at-login toggle registers (check Task Manager → Startup apps).

Document any deviation found and fix it before proceeding (systematic-debugging skill for anything non-obvious).

- [ ] **Step 2.5: Sanity-check on macOS via CI**

Push the branch (`git push -u origin windows-support`) — CI is still single-OS at this point and runs the macOS rust job on the current code, catching any regression in the moved macOS strategies before the CI rewrite in Task 14.

- [ ] **Step 3: Commit any fixes**

```powershell
git add -A; git commit -m "fix(windows): behavior-parity fixes from on-device verification"
```

(Skip the commit if Step 1/2 surfaced nothing.)

---

### Task 14: CI matrix (macOS + Windows)

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Make the rust job a matrix**

Replace the `rust:` job in `.github/workflows/ci.yml` with:

```yaml
  rust:
    name: Rust (${{ matrix.os }})
    strategy:
      fail-fast: false
      matrix:
        os: [macos-14, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: oven-sh/setup-bun@v2
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: src-tauri
      - name: Build frontend (tauri's generate_context! embeds ../dist)
        run: bun install --frozen-lockfile && bun run build
        shell: bash
      - name: Fetch FFmpeg sidecars (used by integration tests)
        run: bun scripts/fetch-ffmpeg.ts
        shell: bash
      - name: rustfmt
        run: cargo fmt --check
        working-directory: src-tauri
      - name: clippy
        run: cargo clippy --all-targets -- -D warnings
        working-directory: src-tauri
      - name: tests
        run: cargo test
        working-directory: src-tauri
```

- [ ] **Step 2: Push and watch**

```powershell
git add .github/workflows/ci.yml; git commit -m "ci: run rust checks on macOS and Windows"
git push -u origin windows-support
gh run watch --exit-status
```

Expected: both matrix legs green (this is the first time the macOS leg compiles the new platform module — fix any macOS-only compile error it reports; you cannot compile macOS locally).

---

### Task 15: Beta prerelease workflow (tag-triggered, branch-friendly)

**Files:**
- Create: `.github/workflows/prerelease.yml`

- [ ] **Step 1: Write the workflow**

```yaml
name: Beta release

# Beta tags (vX.Y.Z-beta.N) can be pushed from any branch: tag-push events run
# the workflow file at the tagged commit, so this needs nothing on main.
on:
  push:
    tags: ["v*-beta*"]

permissions:
  contents: write

jobs:
  create-release:
    name: Create prerelease
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Create GitHub prerelease
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          gh release create "$GITHUB_REF_NAME" --prerelease \
            --title "$GITHUB_REF_NAME" \
            --notes "Beta build. Unsigned binaries — see the README install notes for the Gatekeeper/SmartScreen bypass." \
            || echo "release already exists"

  build:
    name: Build (${{ matrix.os }})
    needs: create-release
    strategy:
      fail-fast: false
      matrix:
        include:
          - os: macos-14
            bundle-glob: src-tauri/target/release/bundle/dmg/*.dmg
          - os: windows-latest
            bundle-glob: src-tauri/target/release/bundle/nsis/*-setup.exe
          - os: windows-11-arm
            bundle-glob: src-tauri/target/release/bundle/nsis/*-setup.exe
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: oven-sh/setup-bun@v2
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: src-tauri
      - run: bun install --frozen-lockfile
        shell: bash
      - run: bun scripts/fetch-ffmpeg.ts
        shell: bash
      - name: Build app bundle
        run: bun tauri build
        shell: bash
      - name: Upload release assets
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        shell: bash
        run: gh release upload "$GITHUB_REF_NAME" ${{ matrix.bundle-glob }} --clobber
```

Notes for the worker: all three are host builds (no `--target`), so bundle paths have no triple segment. NSIS filenames carry the arch (`_x64-setup.exe` / `_arm64-setup.exe`) so the two Windows jobs don't collide; the DMG carries `_aarch64`. If `windows-11-arm` is unavailable for the repo (it requires public repos / newer plans), drop that matrix leg and tell the user the beta will be x64-only (runs emulated on their ARM64 VM).

- [ ] **Step 2: Commit and push**

```powershell
git add .github/workflows/prerelease.yml
git commit -m "ci: tag-triggered beta prerelease pipeline (dmg + nsis x64/arm64)"
git push
```

(Trigger happens in Task 18.)

---

### Task 16: Stable release matrix

**Files:**
- Modify: `.github/workflows/release.yml` (the `build` job, lines 42-65)

- [ ] **Step 1: Replace the single-DMG build job**

```yaml
  build:
    name: Build & publish (${{ matrix.os }})
    needs: version
    if: needs.version.outputs.published == 'true'
    strategy:
      fail-fast: false
      matrix:
        include:
          - os: macos-14
            bundle-glob: src-tauri/target/release/bundle/dmg/*.dmg
          - os: windows-latest
            bundle-glob: src-tauri/target/release/bundle/nsis/*-setup.exe
          - os: windows-11-arm
            bundle-glob: src-tauri/target/release/bundle/nsis/*-setup.exe
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: oven-sh/setup-bun@v2
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: src-tauri
      - run: bun install --frozen-lockfile
        shell: bash
      - run: bun scripts/fetch-ffmpeg.ts
        shell: bash
      - name: Build app bundle
        run: bun tauri build
        shell: bash
      - name: Upload release assets
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          VERSION: ${{ needs.version.outputs.version }}
        shell: bash
        run: gh release upload "v${VERSION}" ${{ matrix.bundle-glob }} --clobber
```

(The macOS job previously passed `--target aarch64-apple-darwin`; `macos-14` runners are Apple Silicon, so the host build produces the same aarch64 DMG — the explicit `cp` to `tamp_${VERSION}_aarch64.dmg` is dropped because Tauri already names the DMG `tamp_<version>_aarch64.dmg`. Verify the produced filename in the beta run from Task 18 before relying on it; adjust with an explicit rename step if it differs.)

- [ ] **Step 2: Commit and push**

```powershell
git add .github/workflows/release.yml
git commit -m "ci: release macOS dmg + windows nsis installers from the changesets flow"
git push
```

---

### Task 17: Documentation + changeset

**Files:**
- Modify: `README.md`, `CONTRIBUTING.md`
- Create: `docs/releasing.md`, `.changeset/windows-support.md`

- [ ] **Step 1: README**

- Tagline/intro: keep macOS phrasing but make the opening paragraph OS-neutral where cheap ("You record your screen, and your OS hands you a 300 MB file…" — keep the ⌘⇧5 mention as the macOS example and add "(or Win+Shift+R on Windows)").
- Features: "MP4 (H.264, hardware-accelerated via VideoToolbox)" → "MP4 (H.264, hardware-accelerated — VideoToolbox on macOS; NVENC/QSV/AMF/Media Foundation on Windows)". Keyboard shortcuts: add "(Ctrl+Alt+T / Ctrl+Alt+O on Windows)" after the ⌘ variants. "reveal any video in Finder" → "reveal any video in Finder/Explorer".
- Install: rename "Download (Apple Silicon)" → "Download — macOS (Apple Silicon)" and add:

```markdown
### Download — Windows

1. Grab `tamp_<version>_x64-setup.exe` (or `_arm64-setup.exe` for Windows on ARM)
   from [**Releases**](https://github.com/ValeriyMaslenikov/tamp/releases/latest).
2. Run it — tamp installs per-user, no admin rights needed.
3. The build is unsigned, so SmartScreen will warn you: click **More info → Run
   anyway**.
4. Look for the compress-arrows icon in the system tray (Windows may tuck it
   behind the ^ overflow — drag it onto the taskbar to keep it visible). tamp
   watches your Desktop and the `Videos\Screen Recordings` / `Videos\Captures`
   folders out of the box.
```

- Build from source: `./scripts/fetch-ffmpeg.sh` → `bun scripts/fetch-ffmpeg.ts`.
- "How it works": "Apple's hardware encoder (VideoToolbox)" → "the OS's hardware encoder (VideoToolbox on macOS; NVENC/QSV/AMF/Media Foundation on Windows)". License footnote: mention BtbN builds alongside martin-riedl.de for the Windows installers.
- Roadmap: replace "Windows & Linux (the architecture is cross-platform; platform shims are isolated)" with "Linux (the platform layer is ready; needs a clipboard/tray strategy and CI target)".

- [ ] **Step 2: CONTRIBUTING**

Prerequisites becomes OS-split:

```markdown
## Prerequisites

- [Rust](https://rustup.rs/) (stable) and [Bun](https://bun.sh/)
- **macOS 12+:** Xcode Command Line Tools (`xcode-select --install`)
- **Windows 10/11:** Visual Studio Build Tools 2022 with the "Desktop
  development with C++" workload (ARM64 component on ARM machines). WebView2 is
  preinstalled on Windows 11.
```

Getting started: `./scripts/fetch-ffmpeg.sh` → `bun scripts/fetch-ffmpeg.ts`. Project layout: `platform/` line becomes "per-OS strategies (clipboard, tray progress, watched folders, hw encoders) behind one trait". Release process: link to `docs/releasing.md`.

- [ ] **Step 3: docs/releasing.md**

```markdown
# Releasing tamp

## Stable releases (changesets, automated)

1. Merged PRs accumulate changeset files in `.changeset/`.
2. The release workflow keeps a **"chore: release"** PR up to date (version
   bump + CHANGELOG). Merging it tags `vX.Y.Z` and creates the GitHub Release.
3. The `build` matrix then attaches: macOS DMG (Apple Silicon), Windows NSIS
   x64, Windows NSIS arm64.

## Beta releases (manual tag, any branch)

Tag-push workflows run the workflow file **at the tagged commit**, so betas
work from feature branches without touching main:

1. On the branch, set a pre-release version in `package.json`
   (e.g. `0.3.0-beta.1`) and commit.
2. `git tag v0.3.0-beta.1 && git push origin v0.3.0-beta.1`
3. The **Beta release** workflow creates a GitHub *prerelease* with the same
   three installers.
4. Before merging the branch, revert the version-bump commit — changesets
   computes the stable version from `package.json`, and a leftover `-beta.N`
   would corrupt the next bump.

## Manual fallback (no CI)

On a machine of the target OS: `bun install && bun scripts/fetch-ffmpeg.ts &&
bun tauri build`, then upload from
`src-tauri/target/release/bundle/{dmg,nsis}/` with
`gh release upload <tag> <file>`. For a different arch than the host, pass
`--target <triple>` to `bun tauri build` and the arch arg to the fetch script
(`arm64`/`x64`) — Windows arm64 cross-builds also need the
`aarch64-pc-windows-msvc` rust target installed.
```

- [ ] **Step 4: Changeset**

Create `.changeset/windows-support.md`:

```markdown
---
"tamp": minor
---

Windows support: tamp now runs in the Windows system tray with the same
size-targeted compression as on macOS — hardware encoding picks from
NVENC/QSV/AMF/Media Foundation with the proven two-pass x264 fallback,
finished files land on the clipboard ready to paste, and releases ship NSIS
installers for x64 and ARM64 alongside the macOS DMG.
```

- [ ] **Step 5: Commit**

```powershell
git add -A; git commit -m "docs: Windows install/build/release documentation + changeset"; git push
```

---

### Task 18: Beta release + on-device validation of the pipeline artifact

**Files:**
- Modify: `package.json` (version only, reverted in Task 19)

- [ ] **Step 1: Bump to beta and tag**

```powershell
# in package.json: "version": "0.2.0" -> "0.3.0-beta.1"
git add package.json; git commit -m "chore: 0.3.0-beta.1"
git tag v0.3.0-beta.1
git push; git push origin v0.3.0-beta.1
```

- [ ] **Step 2: Watch the pipeline**

```powershell
gh run list --workflow "Beta release"; gh run watch --exit-status
```

Expected: create-release + 3 build legs green; `gh release view v0.3.0-beta.1` lists `tamp_0.3.0-beta.1_aarch64.dmg`, `…_x64-setup.exe`, `…_arm64-setup.exe`. Fix-forward any failure (commit, re-tag as `v0.3.0-beta.2` if the tagged commit must change — tags are immutable snapshots).

- [ ] **Step 3: Install the pipeline artifact on this machine**

```powershell
gh release download v0.3.0-beta.1 --pattern "*arm64-setup.exe" --dir "$env:TEMP"
& "$env:TEMP\tamp_0.3.0-beta.1_arm64-setup.exe"
```

(The SmartScreen prompt needs the user's click — tell them.) Then re-run the Task 13 Step 2 manual checklist against the **installed release build** (release builds hide the panel on blur and bundle sidecars — the two things dev runs don't cover).

- [ ] **Step 4: Report**

Summarize results to the user: release URL, what was verified on-device, any platform deviations accepted (e.g. tray ring instead of title text).

---

### Task 19: PR + readability/format review

**Files:**
- Modify: `package.json` (revert beta version)

- [ ] **Step 1: Revert the beta version bump**

```powershell
git revert --no-edit <sha-of-the-0.3.0-beta.1-commit>
git push
```

Expected: `package.json` back at `0.2.0` so changesets bumps cleanly to `0.3.0` on merge.

- [ ] **Step 2: Self-review for OS spaghetti**

```powershell
Select-String -Path src-tauri\src -Pattern 'cfg(target_os|windows)' -Recurse
Select-String -Path src -Pattern 'macos|windows' -Recurse
```

Acceptance: `target_os` appears only in `platform/mod.rs` (module selection), `tray.rs` (`set_title` capability gate), and dependency-level `Cargo.toml`; the frontend branches only inside `src/lib/platform.ts`. Anything else gets refactored into the platform layer now.

- [ ] **Step 3: Final format + tests, then PR**

```powershell
cd src-tauri; cargo fmt; cargo clippy --all-targets -- -D warnings; cargo test; cd ..
bun run test; git add -A
git commit -m "chore: final fmt pass" # only if fmt changed anything
git push
gh pr create --title "Windows support" --body-file - <<'EOF'
## Summary
- Platform trait hierarchy (`src-tauri/src/platform/`): clipboard, tray
  progress, panel config, watched-folder defaults, background-process prep,
  and hardware-encoder candidates each have isolated macOS/Windows strategies
  behind one interface (Linux = one new module).
- Encoder: unchanged size-convergence ladder; hardware attempts now come from
  a probed per-OS candidate list (VideoToolbox / NVENC→QSV→AMF→Media
  Foundation) with fall-through.
- CI: rust checks on macOS + Windows; tag-triggered beta prereleases from any
  branch; stable releases ship DMG + NSIS x64/arm64.
- Verified end-to-end on a Windows 11 ARM64 machine with the pipeline-built
  installer (beta: v0.3.0-beta.1).

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
```

- [ ] **Step 4: Run the code-review skill on the diff and address findings**

Use the `code-review` skill at high effort on the PR; fix legitimate findings (readability, duplication, isolation violations), push, and confirm CI is green.

---

## Self-review notes

- **Spec coverage:** platform trait (T4), clipboard (T6), tray ring (T7), opener (T5), fs4 (T8), watched folders (T9), CREATE_NO_WINDOW (T10), hw candidates + ladder (T11), conf split/NSIS (T12), sidecar fetch (T2), bin.rs triple (T3), CI matrix (T14), beta pipeline (T15), stable matrix (T16), docs+changeset (T17), on-device acceptance + pipeline artifact (T13, T18), PR + spaghetti gate (T19). Frontend accelerator *display* strategy from the spec was dropped deliberately: the preferences UI shows raw accelerator input strings ("CmdOrCtrl+Alt+T"), which are already cross-platform — only `revealLabel` differs (YAGNI).
- **Type consistency:** `HwCandidate { name, extra_args }` and `TrayProgress { fraction, queued }` defined in `platform/mod.rs` (T4) and used with those exact fields in T7/T11; `background_command` defined T4, used T10/T11; `tray::set_title` defined T4 Step 5, called from macOS strategy T4 Step 2.
- **Known uncertainty flagged inline:** `clipboard-win` 5.x exact API (T6), `fs4` free-function name (T8), `windows-11-arm` runner availability (T15), DMG filename in release.yml (T16) — each has a verification step and fallback.
