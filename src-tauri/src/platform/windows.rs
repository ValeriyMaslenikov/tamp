use std::path::PathBuf;

use tauri_plugin_positioner::{Position, WindowExt as _};

use super::{HwCandidate, Platform, TrayProgress};

/// True when the user runs a LIGHT taskbar (Settings → Personalization →
/// Colors → "Choose your mode" set to Light, or "Windows mode" Light). The
/// notification area follows this, not the apps theme, so it's the right
/// signal for tray-icon contrast. Absent/unreadable (older builds, locked-down
/// registry) ⇒ assume dark, tamp's historical default.
fn taskbar_is_light() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|key| key.get_value::<u32, _>("SystemUsesLightTheme"))
        .map(|v| v == 1)
        .unwrap_or(false)
}

/// The tray ink on a dark taskbar (the historical default).
const WHITE_INK: [u8; 3] = [0xff, 0xff, 0xff];
/// The tray ink on a light taskbar, where white would vanish.
const NEAR_BLACK_INK: [u8; 3] = [0x26, 0x26, 0x2b];

/// Ink for the tray glyph, contrasting with the current taskbar: white on the
/// default dark taskbar, near-black on a light one (where white would vanish).
fn tray_ink() -> [u8; 3] {
    if taskbar_is_light() {
        NEAR_BLACK_INK
    } else {
        WHITE_INK
    }
}

/// The dedup key for the tray's DISPLAYED state, so repeated calls that would
/// paint the same icon are skipped. Folds together the encode progress and the
/// taskbar ink (`light`): a live light/dark flip changes the key even while
/// idle, so the ThemeChanged repaint isn't swallowed. Layout keeps every real
/// key clear of the reserved sentinels — bits 0..32 hold the clamped queue
/// depth, bits 32..63 the whole percent (0..=100), and bit 63 is set on a
/// LIGHT taskbar.
///
/// `SEED_KEY` (distinct from any real key) is the pre-first-paint value so the
/// very first call always paints; the two idle keys differ only in the ink bit
/// so an idle theme flip still repaints.
const INK_BIT: u64 = 1 << 63;
const IDLE_DARK: u64 = u64::MAX >> 1; // every progress bit set, ink bit clear
const IDLE_LIGHT: u64 = u64::MAX; // IDLE_DARK with the ink bit set
const SEED_KEY: u64 = IDLE_DARK - 1; // never produced by any progress or idle key
fn tray_dedup_key(progress: Option<TrayProgress>, light: bool) -> u64 {
    let ink_key = if light { INK_BIT } else { 0 };
    match progress {
        None if light => IDLE_LIGHT,
        None => IDLE_DARK,
        Some(p) => {
            ink_key | (u64::from(p.percent()) << 32) | (p.queued.min(u32::MAX as usize) as u64)
        }
    }
}

/// The idle glyph recolored to `ink`. The bundled PNG is authored black for
/// macOS template inversion; Windows has no template support, so we recolor it
/// ourselves. Alpha is preserved, so anti-aliased edges stay smooth.
fn idle_icon(ink: [u8; 3]) -> tauri::image::Image<'static> {
    let base = tauri::include_image!("icons/trayicon.png");
    let mut px = base.rgba().to_vec();
    for p in px.chunks_exact_mut(4) {
        p[0] = ink[0];
        p[1] = ink[1];
        p[2] = ink[2];
    }
    tauri::image::Image::new_owned(px, base.width(), base.height())
}

pub struct Windows;

impl Platform for Windows {
    fn configure_app(&self, _app: &mut tauri::App) {
        // Tray-only presence needs no app-wide tweaks (the panel window
        // already skips the taskbar via its window config).
    }

    /// Writes ALL `paths` as a CF_HDROP file list in one clipboard write, so
    /// pasting into Explorer/Discord/Slack drops the whole set at once.
    fn copy_files_to_clipboard(
        &self,
        _app: &tauri::AppHandle,
        paths: &[PathBuf],
    ) -> Result<(), String> {
        let path_strs = super::paths_to_utf8(paths)?;
        let _clip = clipboard_win::Clipboard::new_attempts(10)
            .map_err(|e| format!("cannot open clipboard: {e}"))?;
        clipboard_win::raw::set_file_list(&path_strs)
            .map_err(|e| format!("clipboard file-list write failed: {e}"))
    }

    fn configure_panel(&self, _window: &tauri::WebviewWindow) -> Result<(), String> {
        // No Spaces/full-screen-auxiliary equivalents to configure.
        Ok(())
    }

    fn position_panel_at_tray(&self, panel: &tauri::WebviewWindow) {
        // Taskbar (and tray) sit at the bottom, so the panel rises ABOVE the
        // icon — TrayCenter anchors the panel's bottom at the icon's top.
        // Constrained keeps a right-edge icon from pushing the centered panel
        // off-screen.
        if panel.move_window_constrained(Position::TrayCenter).is_ok() {
            return;
        }
        // No usable tray rect (e.g. the icon is tucked in the overflow
        // flyout, so no click rect was ever cached): fall back to the work
        // area's bottom corner so the panel is never positioned off-screen.
        crate::log_warn!("no tray rect to anchor the panel; using the work-area corner");
        if let Ok(Some(monitor)) = panel.primary_monitor() {
            self.position_panel_fallback(panel, &monitor);
        }
    }

    fn position_panel_fallback(&self, panel: &tauri::WebviewWindow, monitor: &tauri::Monitor) {
        if let Some(pos) = super::work_area_corner(panel, monitor, super::VAnchor::Bottom) {
            let _ = panel.set_position(pos);
        }
    }

    /// Desktop plus wherever the stock Windows recorders save: Snipping Tool
    /// → Videos\Screen Recordings, Xbox Game Bar → Videos\Captures. These are
    /// watched unconditionally — a recorder creates its folder lazily on the
    /// first capture (often after tamp's first launch), and the scanner
    /// tolerates a not-yet-existent folder, so gating on existence here would
    /// silently miss recordings made later.
    fn default_watched_folders(&self, app: &tauri::AppHandle) -> Vec<PathBuf> {
        let path = tauri::Manager::path(app);
        let mut folders = Vec::new();
        match path.desktop_dir() {
            Ok(desktop) => folders.push(desktop),
            Err(e) => {
                crate::log_warn!("cannot resolve desktop dir for default watched folder: {e}")
            }
        }
        match path.video_dir() {
            Ok(videos) => {
                folders.push(videos.join("Screen Recordings"));
                folders.push(videos.join("Captures"));
            }
            Err(e) => {
                crate::log_warn!("cannot resolve videos dir for default watched folders: {e}")
            }
        }
        folders
    }

    /// tamp is a windows-subsystem app; without CREATE_NO_WINDOW every
    /// ffmpeg/ffprobe spawn flashes a console window over the user's screen.
    fn prepare_background_command(&self, cmd: &mut tokio::process::Command) {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    /// Windows tray icons can't carry text, so progress is a rendered ring
    /// icon plus the exact percentage in the tooltip.
    fn tray_progress(&self, app: &tauri::AppHandle, progress: Option<TrayProgress>) {
        // ffmpeg reports progress every few hundred ms; re-rasterizing the
        // ring and poking the shell is only worth it when the DISPLAYED state
        // actually changed (see `tray_dedup_key`). The key folds in the taskbar
        // ink so a live light/dark flip — which the on_window_event ThemeChanged
        // hook surfaces as a tray_progress(None) repaint — redraws even while
        // idle, instead of being swallowed as "same idle".
        static LAST_SHOWN: std::sync::atomic::AtomicU64 =
            std::sync::atomic::AtomicU64::new(SEED_KEY);
        let ink = tray_ink();
        let shown = tray_dedup_key(progress, ink == WHITE_INK);
        if LAST_SHOWN.swap(shown, std::sync::atomic::Ordering::Relaxed) == shown {
            return;
        }

        let Some(tray) = app.tray_by_id("main") else {
            return;
        };
        let result = match progress {
            Some(p) => {
                let pct = p.percent();
                let tooltip = if p.queued > 0 {
                    format!("Tamp — {pct}% (+{} queued)", p.queued)
                } else {
                    format!("Tamp — {pct}%")
                };
                const SIZE: u32 = 32;
                let icon = tauri::image::Image::new_owned(
                    super::windows_ring::render(p.fraction, SIZE, ink),
                    SIZE,
                    SIZE,
                );
                tray.set_icon(Some(icon))
                    .and_then(|()| tray.set_tooltip(Some(&tooltip)))
            }
            None => tray
                .set_icon(Some(idle_icon(ink)))
                .and_then(|()| tray.set_tooltip(Some("Tamp"))),
        };
        if let Err(e) = result {
            crate::log_warn!("failed to update tray progress: {e}");
        }
    }

    fn hw_candidates(&self) -> &'static [HwCandidate] {
        // Vendor order: dedicated encoders first, Media Foundation (always
        // present, GPU MFT when there is one, software MFT otherwise) last.
        // Availability is probed against the bundled ffmpeg; a candidate
        // that fails at runtime falls through to the next, and overshoot
        // switches to two-pass x264 via the retry ladder.
        &[
            HwCandidate {
                name: "h264_nvenc",
                extra_args: &[],
            },
            HwCandidate {
                name: "h264_qsv",
                extra_args: &[],
            },
            HwCandidate {
                name: "h264_amf",
                extra_args: &[],
            },
            HwCandidate {
                name: "h264_mf",
                extra_args: &[],
            },
        ]
    }

    fn primary_mouse_button_down(&self) -> bool {
        #[link(name = "user32")]
        extern "system" {
            fn GetAsyncKeyState(v_key: i32) -> i16;
        }
        const VK_LBUTTON: i32 = 0x01;
        // High-order bit set ⇒ key is currently down.
        (unsafe { GetAsyncKeyState(VK_LBUTTON) } as u16 & 0x8000) != 0
    }

    fn resolve_accelerator(&self, accelerator: &str) -> String {
        // Windows hotkeys go through the active layout already (RegisterHotKey
        // is virtual-key based), so the configured character works as typed.
        accelerator.to_string()
    }

    fn cleanup_legacy_autostart(&self) {
        // Nothing to clean: the autostart Run value is keyed by the product
        // name "tamp" (rewritten on enable) and the uninstaller drops it; there
        // was never a legacy-identifier variant on Windows. See the macOS impl.
    }
}

#[cfg(test)]
mod tests {
    use super::{tray_dedup_key, SEED_KEY};
    use crate::platform::TrayProgress;

    fn progress(fraction: f64, queued: usize) -> TrayProgress {
        TrayProgress { fraction, queued }
    }

    // manual: on a Windows device, flip Settings → Personalization → Colors
    // → mode while tamp sits idle in the tray; the idle glyph must immediately
    // repaint to contrast the new taskbar (white⇄near-black), not stay
    // invisible until the next encode.
    #[test]
    fn idle_key_changes_with_the_taskbar_ink() {
        // The whole point: an idle theme flip must NOT dedup to the same key,
        // or the ThemeChanged repaint would be swallowed and the glyph would
        // keep the stale ink.
        assert_ne!(
            tray_dedup_key(None, true),
            tray_dedup_key(None, false),
            "idle-light and idle-dark must force a repaint"
        );
    }

    #[test]
    fn same_displayed_state_dedups() {
        // Identical progress + ink ⇒ same key ⇒ the cheap path skips a redraw.
        assert_eq!(
            tray_dedup_key(Some(progress(0.5005, 2)), false),
            tray_dedup_key(Some(progress(0.5004, 2)), false),
            "both round to 50% with the same queue/ink"
        );
    }

    #[test]
    fn progress_ink_and_idle_keys_never_alias_the_seed_or_each_other() {
        // SEED_KEY is the pre-first-paint value; no real key may equal it, or
        // the first paint after seeding would be skipped.
        let mut seen = std::collections::HashSet::new();
        for &light in &[false, true] {
            assert_ne!(tray_dedup_key(None, light), SEED_KEY);
            assert!(
                seen.insert(tray_dedup_key(None, light)),
                "idle keys distinct"
            );
            for pct in 0..=100u32 {
                for &queued in &[0usize, 1, u32::MAX as usize] {
                    let key = tray_dedup_key(Some(progress(pct as f64 / 100.0, queued)), light);
                    assert_ne!(key, SEED_KEY, "progress key aliased the seed");
                    assert_ne!(
                        key,
                        tray_dedup_key(None, light),
                        "progress key aliased an idle key"
                    );
                }
            }
        }
    }

    #[test]
    fn the_same_percent_under_different_ink_differs() {
        assert_ne!(
            tray_dedup_key(Some(progress(0.42, 0)), true),
            tray_dedup_key(Some(progress(0.42, 0)), false),
            "a live theme flip mid-encode must repaint the ring too"
        );
    }
}
