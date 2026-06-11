//! Rotating file logger ({app_log_dir}/tamp.log — ~/Library/Logs/<identifier>
//! on macOS).
//!
//! The current file is `tamp.log`; when a write would push it past
//! [`MAX_FILE_BYTES`] it rotates to `tamp.1.log`, `tamp.2.log`, … keeping at
//! most [`MAX_ROTATED`] rotated files (the oldest is dropped), so the whole
//! set stays around 10 MB. Everything is best-effort: a logging failure must
//! never break the app, so write errors are swallowed. Debug builds mirror
//! every line to stderr for `cargo tauri dev` visibility.
//!
//! tauri-plugin-log v2 was considered (its `RotationStrategy::KeepSome`
//! bounds the file count) but it renames rotated files to dated names and
//! its rotation is neither configurable for tests nor expressible as the
//! `tamp.N.log` scheme, so this small module exists instead.
//!
//! Use the [`log_debug!`](crate::log_debug)/[`log_info!`](crate::log_info)/
//! [`log_warn!`](crate::log_warn)/[`log_error!`](crate::log_error) macros;
//! they fill in the calling module as the target.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

/// The current log file; rotations move it to `tamp.1.log` … `tamp.N.log`.
const LOG_FILE: &str = "tamp.log";
/// Rotate when a write would push the current file past this size (~2 MB).
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
/// Rotated files kept (`tamp.1.log` … `tamp.4.log`); the oldest is dropped.
const MAX_ROTATED: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    fn as_str(self) -> &'static str {
        match self {
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
}

struct Writer {
    /// `None` only when (re)opening the current file failed; writes are
    /// dropped until the next rotation retries the open.
    file: Option<File>,
    /// Bytes written to the current file (seeded from its size on open).
    written: u64,
}

/// A size-rotating line writer over `dir/tamp.log`. The global logger wraps
/// one; tests construct their own with tiny limits.
pub struct RotatingLogger {
    dir: PathBuf,
    max_file_bytes: u64,
    max_rotated: usize,
    writer: Mutex<Writer>,
}

impl RotatingLogger {
    fn create(dir: PathBuf, max_file_bytes: u64, max_rotated: usize) -> std::io::Result<Self> {
        std::fs::create_dir_all(&dir)?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(LOG_FILE))?;
        let written = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(RotatingLogger {
            dir,
            max_file_bytes,
            max_rotated,
            writer: Mutex::new(Writer {
                file: Some(file),
                written,
            }),
        })
    }

    /// Appends `line` (plus a newline), rotating first when the write would
    /// push the current file past the size limit. A line bigger than the
    /// whole limit still lands in a fresh file rather than rotating forever.
    fn write_line(&self, line: &str) {
        let mut writer = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        let len = line.len() as u64 + 1;
        if writer.written > 0 && writer.written + len > self.max_file_bytes {
            self.rotate(&mut writer);
        }
        if let Some(file) = writer.file.as_mut() {
            let ok = file
                .write_all(line.as_bytes())
                .and_then(|()| file.write_all(b"\n"))
                .is_ok();
            if ok {
                writer.written += len;
            }
        }
    }

    /// `tamp.log` -> `tamp.1.log` -> … -> `tamp.{max_rotated}.log` -> dropped.
    fn rotate(&self, writer: &mut Writer) {
        writer.file = None; // close the current file before renaming it
        let _ = std::fs::remove_file(self.rotated_path(self.max_rotated));
        for i in (1..self.max_rotated).rev() {
            let from = self.rotated_path(i);
            if from.exists() {
                let _ = std::fs::rename(&from, self.rotated_path(i + 1));
            }
        }
        let current = self.dir.join(LOG_FILE);
        let _ = std::fs::rename(&current, self.rotated_path(1));
        writer.written = 0;
        writer.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&current)
            .ok();
    }

    fn rotated_path(&self, n: usize) -> PathBuf {
        self.dir.join(format!("tamp.{n}.log"))
    }
}

static LOGGER: OnceLock<RotatingLogger> = OnceLock::new();

/// Initializes the global logger writing to `dir/tamp.log` with the standard
/// limits. Later calls are no-ops. On error the app keeps running without a
/// log file (debug builds still mirror to stderr).
pub fn init(dir: PathBuf) -> std::io::Result<()> {
    let logger = RotatingLogger::create(dir, MAX_FILE_BYTES, MAX_ROTATED)?;
    let _ = LOGGER.set(logger);
    Ok(())
}

/// Writes one timestamped line to the log file (when [`init`] succeeded) and
/// mirrors it to stderr in debug builds. Prefer the `log_*!` macros, which
/// pass the calling module as `target`.
pub fn log(level: Level, target: &str, message: &str) {
    let line = format!(
        "{} {:5} {target}: {message}",
        format_timestamp(SystemTime::now()),
        level.as_str()
    );
    #[cfg(debug_assertions)]
    eprintln!("{line}");
    if let Some(logger) = LOGGER.get() {
        logger.write_line(&line);
    }
}

/// RFC3339-ish UTC timestamp (`2026-06-11T19:03:12.345Z`) from std::time
/// alone — chrono/time are deliberately not in the tree.
fn format_timestamp(now: SystemTime) -> String {
    let since_epoch = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = since_epoch.as_secs();
    let millis = since_epoch.subsec_millis();
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (h, m, s) = (rem / 3_600, (rem % 3_600) / 60, rem % 60);
    let (year, month, day) = civil_from_days(days as i64);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}.{millis:03}Z")
}

/// Days-since-epoch to (year, month, day) in the proleptic Gregorian
/// calendar — Howard Hinnant's `civil_from_days` algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // day of era [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // year of era
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year (Mar 1 based)
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Logs at DEBUG with the calling module as the target.
#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)+) => {
        $crate::logging::log($crate::logging::Level::Debug, module_path!(), &format!($($arg)+))
    };
}

/// Logs at INFO with the calling module as the target.
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)+) => {
        $crate::logging::log($crate::logging::Level::Info, module_path!(), &format!($($arg)+))
    };
}

/// Logs at WARN with the calling module as the target.
#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)+) => {
        $crate::logging::log($crate::logging::Level::Warn, module_path!(), &format!($($arg)+))
    };
}

/// Logs at ERROR with the calling module as the target.
#[macro_export]
macro_rules! log_error {
    ($($arg:tt)+) => {
        $crate::logging::log($crate::logging::Level::Error, module_path!(), &format!($($arg)+))
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn read(dir: &std::path::Path, name: &str) -> Option<String> {
        std::fs::read_to_string(dir.join(name)).ok()
    }

    #[test]
    fn lines_accumulate_without_rotation_below_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let logger = RotatingLogger::create(dir.path().to_path_buf(), 1024, 2).unwrap();
        logger.write_line("first");
        logger.write_line("second");
        assert_eq!(read(dir.path(), "tamp.log").unwrap(), "first\nsecond\n");
        assert!(read(dir.path(), "tamp.1.log").is_none());
    }

    #[test]
    fn writes_past_the_limit_rotate_capped_and_drop_the_oldest() {
        let dir = tempfile::tempdir().unwrap();
        // 10-byte limit, 2 rotated files kept. Each 8-byte line (+newline)
        // forces a rotation on the NEXT write, so every line lands in its
        // own file generation.
        let logger = RotatingLogger::create(dir.path().to_path_buf(), 10, 2).unwrap();
        for i in 0..4 {
            logger.write_line(&format!("line-{i:03}"));
        }
        // Newest in the current file, older down the chain, oldest dropped.
        assert_eq!(read(dir.path(), "tamp.log").unwrap(), "line-003\n");
        assert_eq!(read(dir.path(), "tamp.1.log").unwrap(), "line-002\n");
        assert_eq!(read(dir.path(), "tamp.2.log").unwrap(), "line-001\n");
        assert!(
            read(dir.path(), "tamp.3.log").is_none(),
            "the rotated-file count must stay capped"
        );
        let log_files = std::fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(log_files, 3, "current file + 2 rotated, nothing else");
    }

    #[test]
    fn an_oversized_line_still_lands_in_a_fresh_file() {
        let dir = tempfile::tempdir().unwrap();
        let logger = RotatingLogger::create(dir.path().to_path_buf(), 10, 2).unwrap();
        let huge = "x".repeat(50); // bigger than the whole file limit
        logger.write_line(&huge);
        assert_eq!(read(dir.path(), "tamp.log").unwrap(), format!("{huge}\n"));
        // And the next ordinary write rotates it away instead of growing it.
        logger.write_line("after");
        assert_eq!(read(dir.path(), "tamp.log").unwrap(), "after\n");
        assert_eq!(read(dir.path(), "tamp.1.log").unwrap(), format!("{huge}\n"));
    }

    #[test]
    fn reopening_resumes_the_current_file_and_its_size() {
        let dir = tempfile::tempdir().unwrap();
        {
            let logger = RotatingLogger::create(dir.path().to_path_buf(), 20, 2).unwrap();
            logger.write_line("0123456789"); // 11 bytes with the newline
        }
        // A new logger (app restart) must count the existing bytes: the next
        // 11-byte line would exceed 20, so it rotates first.
        let logger = RotatingLogger::create(dir.path().to_path_buf(), 20, 2).unwrap();
        logger.write_line("abcdefghij");
        assert_eq!(read(dir.path(), "tamp.log").unwrap(), "abcdefghij\n");
        assert_eq!(read(dir.path(), "tamp.1.log").unwrap(), "0123456789\n");
    }

    #[test]
    fn timestamps_format_rfc3339_utc() {
        assert_eq!(format_timestamp(UNIX_EPOCH), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            format_timestamp(UNIX_EPOCH + Duration::from_millis(1_700_000_000_123)),
            "2023-11-14T22:13:20.123Z"
        );
        // Leap day.
        assert_eq!(
            format_timestamp(UNIX_EPOCH + Duration::from_secs(1_709_164_800)),
            "2024-02-29T00:00:00.000Z"
        );
    }
}
