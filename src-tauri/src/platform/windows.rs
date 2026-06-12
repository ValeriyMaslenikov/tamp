use std::path::PathBuf;

use super::{HwCandidate, Platform, TrayProgress};

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

    fn configure_panel(&self, _window: &tauri::WebviewWindow) -> Result<(), String> {
        // No Spaces/full-screen-auxiliary equivalents to configure.
        Ok(())
    }

    /// Desktop plus wherever the stock Windows recorders save: Snipping Tool
    /// → Videos\Screen Recordings, Xbox Game Bar → Videos\Captures. The
    /// Videos subfolders are only watched when they exist (they appear after
    /// first use of the respective tool); Desktop is watched unconditionally.
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
            Ok(videos) => folders.extend(
                ["Screen Recordings", "Captures"]
                    .iter()
                    .map(|sub| videos.join(sub))
                    .filter(|dir| dir.is_dir()),
            ),
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
                let icon = tauri::image::Image::new_owned(
                    super::windows_ring::render(p.fraction, SIZE),
                    SIZE,
                    SIZE,
                );
                tray.set_icon(Some(icon))
                    .and_then(|()| tray.set_tooltip(Some(&tooltip)))
            }
            None => tray
                .set_icon(Some(tauri::include_image!("icons/trayicon.png")))
                .and_then(|()| tray.set_tooltip(Some("tamp"))),
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
}
