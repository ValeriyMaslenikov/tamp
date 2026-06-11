pub mod bin;
pub mod plan;
pub mod probe;
pub mod progress;
pub mod retry;

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

// Re-exported so integration tests can build presets even though the
// `settings` module itself is private to the crate.
pub use crate::settings::{OutputFormat, Preset};

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobState {
    pub id: String,
    pub input_path: String,
    pub input_name: String,
    pub output_path: Option<String>,
    pub preset_id: String,
    /// `plan::preset_hash` of the job's preset configuration, so the
    /// trash-original guard and the frontend can tell whether two jobs for
    /// the same input actually share an encode configuration.
    pub preset_hash: String,
    pub phase: Phase,
    pub progress: f64, // 0..1 overall
    pub input_bytes: u64,
    pub output_bytes: Option<u64>,
    pub error: Option<String>,
    /// Set when a post-action (clipboard copy / trash original) fails after a
    /// successful encode; `phase` stays `Done`.
    pub post_error: Option<String>,
    /// True when the encode was skipped because an output for this exact
    /// input + preset configuration already existed and was reused.
    pub reused: bool,
}

#[derive(Clone, Copy, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Queued,
    Pass1,
    Pass2,
    Verifying,
    Done,
    Failed,
    Cancelled,
}

impl Phase {
    fn is_terminal(self) -> bool {
        matches!(self, Phase::Done | Phase::Failed | Phase::Cancelled)
    }
}

pub struct PostActions {
    pub copy_to_clipboard: bool,
    pub trash_original: bool,
}

/// Shared slot holding the currently running ffmpeg child so `cancel()` /
/// `shutdown()` can kill it from another thread.
pub type ChildSlot = Arc<Mutex<Option<tokio::process::Child>>>;

const MAX_TRACKED_JOBS: usize = 50;
const EMIT_THROTTLE: Duration = Duration::from_millis(100);

struct QueuedJob {
    id: String,
    input: PathBuf,
    preset: Preset,
    post: PostActions,
    use_hardware: bool,
}

struct Inner {
    app: AppHandle,
    tx: mpsc::UnboundedSender<QueuedJob>,
    jobs: Mutex<Vec<JobState>>,
    cancelled: Mutex<HashSet<String>>,
    current_id: Mutex<Option<String>>,
    child_slot: ChildSlot,
    /// Jobs waiting in the channel (excludes the one being processed).
    pending: AtomicUsize,
    last_emit: Mutex<Option<Instant>>,
    /// Set by `shutdown()` BEFORE the child is killed: the worker folds it
    /// into its is_cancelled checks so the kill reads as a cancel — no
    /// VT->software fallback and no further convergence retries on exit.
    shutting_down: AtomicBool,
}

pub struct Encoder {
    inner: Arc<Inner>,
}

impl Encoder {
    pub fn start(app: AppHandle) -> Encoder {
        let (tx, mut rx) = mpsc::unbounded_channel::<QueuedJob>();
        let inner = Arc::new(Inner {
            app,
            tx,
            jobs: Mutex::new(Vec::new()),
            cancelled: Mutex::new(HashSet::new()),
            current_id: Mutex::new(None),
            child_slot: ChildSlot::default(),
            pending: AtomicUsize::new(0),
            last_emit: Mutex::new(None),
            shutting_down: AtomicBool::new(false),
        });
        let worker = inner.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(job) = rx.recv().await {
                worker.pending.fetch_sub(1, Ordering::SeqCst);
                process_job(&worker, job).await;
            }
        });
        Encoder { inner }
    }

    pub fn enqueue(
        &self,
        input: PathBuf,
        preset: Preset,
        post: PostActions,
        use_hardware: bool,
    ) -> Result<String, String> {
        let meta = std::fs::metadata(&input).map_err(|e| format!("cannot read input file: {e}"))?;
        if !meta.is_file() {
            return Err("input is not a file".to_string());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let state = JobState {
            id: id.clone(),
            input_path: input.to_string_lossy().into_owned(),
            input_name: input
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            output_path: None,
            preset_id: preset.id.clone(),
            preset_hash: plan::preset_hash(&preset),
            phase: Phase::Queued,
            progress: 0.0,
            input_bytes: meta.len(),
            output_bytes: None,
            error: None,
            post_error: None,
            reused: false,
        };
        crate::log_info!(
            "job {id}: enqueued {} ({} bytes) with preset \"{}\" (hash {}, format {:?}, target {} MB)",
            state.input_path,
            state.input_bytes,
            preset.name,
            state.preset_hash,
            preset.format,
            preset.target_mb
        );
        {
            let mut jobs = self.inner.jobs.lock().unwrap();
            // Evict terminal jobs only: dropping a running/queued job would
            // silence its terminal event, skip partial-output cleanup, and
            // orphan its post-actions. If every tracked job is still active,
            // let the list grow temporarily — later enqueues trim it back.
            while jobs.len() >= MAX_TRACKED_JOBS {
                match jobs.iter().position(|j| j.phase.is_terminal()) {
                    Some(pos) => {
                        jobs.remove(pos);
                    }
                    None => break,
                }
            }
            jobs.push(state.clone());
        }
        emit_state(&self.inner, &state, true);
        self.inner.pending.fetch_add(1, Ordering::SeqCst);
        if self
            .inner
            .tx
            .send(QueuedJob {
                id: id.clone(),
                input,
                preset,
                post,
                use_hardware,
            })
            .is_err()
        {
            self.inner.pending.fetch_sub(1, Ordering::SeqCst);
            return Err("encoder worker is not running".to_string());
        }
        Ok(id)
    }

    pub fn cancel(&self, id: &str) {
        crate::log_info!("job {id}: cancel requested");
        self.inner.cancelled.lock().unwrap().insert(id.to_string());
        let is_current = self.inner.current_id.lock().unwrap().as_deref() == Some(id);
        if is_current {
            if let Some(child) = self.inner.child_slot.lock().unwrap().as_mut() {
                let _ = child.start_kill();
            }
        } else if let Some(state) = update_job(&self.inner, id, |j| {
            if j.phase == Phase::Queued {
                j.phase = Phase::Cancelled;
            }
        }) {
            // Queued job: flip to Cancelled right away; the worker skips it later.
            if state.phase == Phase::Cancelled {
                emit_state(&self.inner, &state, true);
            }
        }
    }

    pub fn snapshot(&self) -> Vec<JobState> {
        self.inner.jobs.lock().unwrap().clone()
    }

    pub fn shutdown(&self) {
        // Flag first: the worker must already see the shutdown when the kill
        // surfaces as an ffmpeg error, or it would fall back to software /
        // burn convergence retries while the app is exiting.
        self.inner.shutting_down.store(true, Ordering::SeqCst);
        if let Some(child) = self.inner.child_slot.lock().unwrap().as_mut() {
            let _ = child.start_kill();
        }
    }
}

fn update_job(inner: &Inner, id: &str, f: impl FnOnce(&mut JobState)) -> Option<JobState> {
    let mut jobs = inner.jobs.lock().unwrap();
    let job = jobs.iter_mut().find(|j| j.id == id)?;
    f(job);
    Some(job.clone())
}

fn emit_state(inner: &Inner, state: &JobState, force: bool) {
    {
        let mut last = inner.last_emit.lock().unwrap();
        if !force && last.is_some_and(|t| t.elapsed() < EMIT_THROTTLE) {
            return;
        }
        *last = Some(Instant::now());
    }
    if let Err(e) = inner.app.emit("encode:state", state) {
        crate::log_warn!("failed to emit encode:state: {e}");
    }
}

fn update_tray(inner: &Inner, progress: Option<f64>) {
    let text = progress.map(|p| {
        let pct = (p.clamp(0.0, 1.0) * 100.0).round() as u32;
        let queued = inner.pending.load(Ordering::SeqCst);
        if queued > 0 {
            format!("{pct}% (+{queued})")
        } else {
            format!("{pct}%")
        }
    });
    crate::tray::set_progress(&inner.app, text);
}

async fn process_job(inner: &Arc<Inner>, job: QueuedJob) {
    if inner.cancelled.lock().unwrap().contains(&job.id) {
        crate::log_info!("job {}: cancelled while queued", job.id);
        if let Some(state) = update_job(inner, &job.id, |j| j.phase = Phase::Cancelled) {
            emit_state(inner, &state, true);
        }
        if inner.pending.load(Ordering::SeqCst) == 0 {
            crate::tray::set_progress(&inner.app, None);
        }
        return;
    }

    crate::log_info!("job {}: starting", job.id);
    *inner.current_id.lock().unwrap() = Some(job.id.clone());
    let outcome = run_job(inner, &job).await;
    *inner.current_id.lock().unwrap() = None;

    if let Err(err) = outcome {
        let was_cancelled = inner.cancelled.lock().unwrap().contains(&job.id);
        let partial = inner
            .jobs
            .lock()
            .unwrap()
            .iter()
            .find(|j| j.id == job.id)
            .and_then(|j| j.output_path.clone());
        if let Some(path) = partial {
            let path = Path::new(&path);
            if path.exists() {
                let _ = std::fs::remove_file(path);
            }
            // The in-flight attempt writes to the part sibling, so failure
            // and cancellation must clear that too.
            let part = plan::part_path(path);
            if part.exists() {
                let _ = std::fs::remove_file(&part);
            }
        }
        if was_cancelled {
            crate::log_info!("job {}: cancelled", job.id);
        } else {
            crate::log_error!("job {} failed: {err}", job.id);
        }
        if let Some(state) = update_job(inner, &job.id, |j| {
            if was_cancelled {
                j.phase = Phase::Cancelled;
                j.output_path = None;
                j.error = None;
            } else {
                j.phase = Phase::Failed;
                j.error = Some(err.clone());
            }
        }) {
            emit_state(inner, &state, true);
        }
    }

    if inner.pending.load(Ordering::SeqCst) == 0 {
        crate::tray::set_progress(&inner.app, None);
    }
}

async fn run_job(inner: &Arc<Inner>, job: &QueuedJob) -> Result<(), String> {
    let preset_hash = plan::preset_hash(&job.preset);
    let target_bytes = job.preset.target_mb * 1_000_000.0;

    // A previous run with this exact preset configuration already produced
    // an output next to the input: reuse it instead of re-encoding — but
    // only when it actually fits the target AND the journal doesn't show it
    // was made from different content (another input path, or an input
    // re-recorded since). An over-target file (stale artifact of the old
    // keep-it policy) is deleted by check_reuse, BEFORE build_plan's
    // unique_output probe, so the fresh encode targets the now-free base
    // expected_output path; an under-target file with mismatched provenance
    // is left alone and the fresh encode lands on a numbered sibling.
    let expected = plan::expected_output(&job.input, &preset_hash, job.preset.format);
    if let Some(output_bytes) = check_reuse(&expected, target_bytes) {
        if reuse_is_stale(&inner.app, &job.input, &expected) {
            crate::log_info!(
                "job {}: not reusing {} — the journal records different content; re-encoding",
                job.id,
                expected.display()
            );
        } else {
            crate::log_info!(
                "job {}: reusing existing output {} ({output_bytes} bytes)",
                job.id,
                expected.display()
            );
            // Post-actions still run so the reuse behaves like a fresh encode
            // from the user's perspective (clipboard copy / trash original).
            let post_error = run_post_actions(inner, &job.post, &job.input, &expected);
            if let Some(state) = update_job(inner, &job.id, |j| {
                j.phase = Phase::Done;
                j.progress = 1.0;
                j.reused = true;
                j.output_path = Some(expected.to_string_lossy().into_owned());
                j.output_bytes = Some(output_bytes);
                j.post_error = post_error;
            }) {
                emit_state(inner, &state, true);
            }
            // Deliberately no journal append: the original encode already
            // recorded this output.
            return Ok(());
        }
    }

    let info = probe::probe(&job.input).await?;
    let mut plan = plan::build_plan(&info, &job.preset, &job.input)?;
    let tmp = tempfile::tempdir().map_err(|e| format!("cannot create temp dir: {e}"))?;

    if let Some(state) = update_job(inner, &job.id, |j| {
        j.phase = Phase::Pass1;
        j.output_path = Some(plan.output.to_string_lossy().into_owned());
    }) {
        emit_state(inner, &state, true);
    }
    update_tray(inner, Some(0.0));

    let cb_inner = inner.clone();
    let cb_id = job.id.clone();
    let mut last_phase = Phase::Pass1;
    let mut on_progress = move |pass: u8, overall: f64| {
        let phase = if pass <= 1 {
            Phase::Pass1
        } else {
            Phase::Pass2
        };
        let force = phase != last_phase;
        last_phase = phase;
        if let Some(state) = update_job(&cb_inner, &cb_id, |j| {
            j.phase = phase;
            j.progress = overall;
        }) {
            emit_state(&cb_inner, &state, force);
        }
        update_tray(&cb_inner, Some(overall));
    };

    let c_inner = inner.clone();
    let c_id = job.id.clone();
    // shutdown() reads as a cancel at every decision point: no VT->software
    // fallback and no further retries once the app is exiting.
    let is_cancelled = move || {
        c_inner.shutting_down.load(Ordering::SeqCst)
            || c_inner.cancelled.lock().unwrap().contains(&c_id)
    };

    // GIF has its own engine: palette-encode attempts that shrink the width
    // (and eventually the frame rate) until the target fits; run_gif FAILS —
    // deleting its output — when the bounded retries can't get there, so an
    // oversized gif is never delivered.
    let actual = if job.preset.format == OutputFormat::Gif {
        run_gif(
            &plan,
            &info,
            &job.input,
            target_bytes,
            &inner.child_slot,
            &is_cancelled,
            &mut on_progress,
        )
        .await?
    } else {
        // WebM is always software two-pass VP9 — VideoToolbox has no VP9
        // encoder. The convergence loop handles oversize retries and the
        // hardware -> software switch, and fails rather than ever returning
        // an over-target output.
        let use_hardware = job.use_hardware && job.preset.format == OutputFormat::Mp4;
        run_video_convergence(
            &mut plan,
            &info,
            &job.input,
            tmp.path(),
            target_bytes,
            use_hardware,
            &inner.child_slot,
            &is_cancelled,
            &mut on_progress,
        )
        .await?
    };

    if let Some(state) = update_job(inner, &job.id, |j| {
        j.phase = Phase::Verifying;
        j.progress = 1.0;
    }) {
        emit_state(inner, &state, true);
    }

    // Defense in depth: the engines above already enforced the target on the
    // part file before renaming it onto plan.output, but a Done job must
    // NEVER carry an over-target output, so re-verify the delivered file one
    // last time and fail (removing it) on any violation.
    let actual = enforce_target(&plan.output, actual, target_bytes, job.preset.target_mb)?;

    crate::log_info!(
        "job {}: done — {} -> {} ({actual} bytes, target {} MB)",
        job.id,
        job.input.display(),
        plan.output.display(),
        job.preset.target_mb
    );
    // Run post-actions first so their failures ride along on the final Done
    // state instead of vanishing into stderr.
    let post_error = run_post_actions(inner, &job.post, &job.input, &plan.output);
    append_journal(inner, job, &plan.output, &preset_hash, actual);
    if let Some(state) = update_job(inner, &job.id, |j| {
        j.phase = Phase::Done;
        j.progress = 1.0;
        j.output_bytes = Some(actual);
        j.post_error = post_error;
    }) {
        emit_state(inner, &state, true);
    }
    Ok(())
}

/// The reuse short-circuit decision for the worker: `Some(bytes)` when an
/// existing output at `expected` fits `target_bytes` and can be served
/// as-is; `None` when there is nothing (usable) to reuse. Only a non-empty
/// REGULAR file is ever served — a directory or symlink squatting on the
/// expected name is not tamp's to deliver or delete, so it is left alone
/// (unique_output sends the fresh encode to a numbered sibling). An
/// over-target file — a stale artifact of the old keep-it policy — or a
/// zero-byte one IS tamp's own hash-named artifact and is deleted here to
/// free the base output path; nothing in this probe ever fails the job, a
/// failed deletion is logged and the encode proceeds to a sibling.
pub fn check_reuse(expected: &Path, target_bytes: f64) -> Option<u64> {
    let Ok(meta) = std::fs::symlink_metadata(expected) else {
        return None; // nothing there (or unreadable): encode fresh
    };
    if !meta.is_file() {
        crate::log_warn!(
            "expected output {} is not a regular file; leaving it and encoding to a sibling",
            expected.display()
        );
        sweep_stale_siblings(expected, target_bytes);
        return None;
    }
    let len = meta.len();
    if len > 0 && len as f64 <= target_bytes {
        return Some(len);
    }
    if let Err(e) = std::fs::remove_file(expected) {
        crate::log_warn!(
            "cannot remove stale output {}: {e}; encoding to a sibling",
            expected.display()
        );
    }
    if len as f64 > target_bytes {
        sweep_stale_siblings(expected, target_bytes);
    }
    None
}

/// Best-effort sweep of stale numbered siblings — "{stem} (tamped {hash} N).{ext}"
/// outputs of the SAME input + preset configuration as `expected` — deleting
/// those over `target_bytes`. Runs when check_reuse retires a stale base
/// output so old over-target artifacts don't live on forever under numbered
/// names; every failure is logged and skipped, never fatal.
fn sweep_stale_siblings(expected: &Path, target_bytes: f64) {
    let (Some(dir), Some(base_stem), Some(ext)) = (
        expected.parent(),
        expected.file_stem().and_then(|s| s.to_str()),
        expected.extension().and_then(|e| e.to_str()),
    ) else {
        return;
    };
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            crate::log_warn!("cannot scan {} for stale siblings: {e}", dir.display());
            return;
        }
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !plan::is_numbered_sibling(name, base_stem, ext) {
            continue;
        }
        // DirEntry::metadata does not traverse symlinks, so only a regular
        // over-target file is ever swept.
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() || meta.len() as f64 <= target_bytes {
            continue;
        }
        if let Err(e) = std::fs::remove_file(entry.path()) {
            crate::log_warn!(
                "cannot remove stale over-target sibling {}: {e}",
                entry.path().display()
            );
        }
    }
}

/// Provenance gate for the reuse short-circuit: true when the journal shows
/// the file at `expected` was produced from something OTHER than this job's
/// input — recorded for a different input path, or the input was modified
/// (re-recorded) after the record was written. No managed journal or no
/// record keeps the size-only reuse behavior.
fn reuse_is_stale(app: &AppHandle, input: &Path, expected: &Path) -> bool {
    let Some(journal) = app.try_state::<crate::journal::Journal>() else {
        return false;
    };
    let Some(record) = journal.find_by_output(&expected.to_string_lossy()) else {
        return false;
    };
    if record.input_path != input.to_string_lossy() {
        return true;
    }
    std::fs::metadata(input)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .is_some_and(|d| d.as_millis() as u64 > record.completed_at_ms)
}

/// Final guarantee before a job goes Done: re-stat the output and confirm it
/// fits the byte target. On violation the file is removed and the job fails
/// with the actionable error — an oversized file must never be delivered.
fn enforce_target(
    output: &Path,
    expected_len: u64,
    target_bytes: f64,
    target_mb: f64,
) -> Result<u64, String> {
    let actual = std::fs::metadata(output)
        .map_err(|e| format!("encoded output missing: {e}"))?
        .len();
    if actual as f64 > target_bytes || expected_len as f64 > target_bytes {
        let _ = std::fs::remove_file(output);
        return Err(retry::give_up_error(target_mb));
    }
    Ok(actual)
}

/// The explicit ffmpeg muxer for a plan's container. Encode attempts write
/// to the ".part" sibling, so ffmpeg can't infer the format from the output
/// extension and must always be told.
fn muxer(format: OutputFormat) -> &'static str {
    match format {
        OutputFormat::Mp4 => "mp4",
        OutputFormat::Webm => "webm",
        OutputFormat::Gif => "gif",
    }
}

/// Promotes a finished part file onto its final output path: enforce_target
/// re-stats the part and removes it on any size violation, and only a
/// fitting file is renamed — same-directory, so the rename is atomic and a
/// crash can never leave a partial at the final path.
fn promote_part(
    part: &Path,
    output: &Path,
    expected_len: u64,
    target_bytes: f64,
) -> Result<u64, String> {
    let actual = enforce_target(part, expected_len, target_bytes, target_bytes / 1_000_000.0)?;
    if let Err(e) = std::fs::rename(part, output) {
        let _ = std::fs::remove_file(part);
        return Err(format!("cannot move finished output into place: {e}"));
    }
    Ok(actual)
}

/// Runs the bitrate-convergence loop for mp4/webm plans: encode, measure,
/// then per `retry::next_action` either deliver, re-encode with a corrected
/// bitrate (a VideoToolbox job switches to two-pass software on its FIRST
/// overshoot; software jobs just tighten per the shrink schedule), or fail
/// with an actionable error after the bounded retries. Every attempt writes
/// to the crash-safe `plan::part_path` sibling; a fitting output is verified
/// (enforce_target) and only then renamed onto `plan.output`, while every
/// failure/cancel/give-up path removes the part file — a partial can never
/// land at the final path. Returns the final output size, which is always
/// <= `target_bytes`. Public so the integration test can exercise the exact
/// loop the worker uses.
#[allow(clippy::too_many_arguments)]
pub async fn run_video_convergence(
    plan: &mut plan::EncodePlan,
    info: &probe::ProbeInfo,
    input: &Path,
    passlog_dir: &Path,
    target_bytes: f64,
    use_hardware: bool,
    child_slot: &ChildSlot,
    is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    on_progress: &mut (dyn FnMut(u8, f64) + Send),
) -> Result<u64, String> {
    let mut work = plan.clone();
    work.output = plan::part_path(&plan.output);
    let result = convergence_attempts(
        &mut work,
        info,
        input,
        passlog_dir,
        target_bytes,
        use_hardware,
        child_slot,
        is_cancelled,
        on_progress,
    )
    .await;
    plan.video_kbit = work.video_kbit; // expose the corrected bitrate
    match result {
        Ok(actual) => promote_part(&work.output, &plan.output, actual, target_bytes),
        Err(e) => {
            let _ = std::fs::remove_file(&work.output);
            Err(e)
        }
    }
}

/// The encode/measure/correct loop behind [`run_video_convergence`];
/// `plan.output` already points at the part sibling. Returns the fitting
/// attempt's size, leaving the file at the part path for the caller to
/// verify and promote.
#[allow(clippy::too_many_arguments)]
async fn convergence_attempts(
    plan: &mut plan::EncodePlan,
    info: &probe::ProbeInfo,
    input: &Path,
    passlog_dir: &Path,
    target_bytes: f64,
    use_hardware: bool,
    child_slot: &ChildSlot,
    is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    on_progress: &mut (dyn FnMut(u8, f64) + Send),
) -> Result<u64, String> {
    let mut use_hardware = use_hardware && plan.format == OutputFormat::Mp4;
    for attempt in 0..=retry::MAX_RE_ENCODES {
        if use_hardware {
            let hw =
                run_hardware_pass(plan, info, input, child_slot, is_cancelled, on_progress).await;
            let fallback_reason = match hw {
                Ok(()) => match std::fs::metadata(&plan.output) {
                    Ok(meta) if meta.len() > 0 => None,
                    _ => Some("hardware encode produced no output".to_string()),
                },
                Err(e) if is_cancelled() => return Err(e),
                Err(e) => Some(e),
            };
            if let Some(reason) = fallback_reason {
                crate::log_warn!("{reason}; falling back to two-pass libx264");
                use_hardware = false; // sticks for every later attempt too
            }
        }
        if !use_hardware {
            // run_passes restarts progress from 0 on its own first callback.
            run_passes(
                plan,
                info,
                input,
                passlog_dir,
                child_slot,
                is_cancelled,
                on_progress,
            )
            .await?;
        }

        let actual = std::fs::metadata(&plan.output)
            .map_err(|e| format!("encoded output missing: {e}"))?
            .len();
        let encoder = if use_hardware {
            retry::EncoderKind::Hardware
        } else {
            retry::EncoderKind::Software
        };
        match retry::next_action(plan.video_kbit, actual, target_bytes, attempt, encoder) {
            retry::NextAction::Done => {
                crate::log_info!(
                    "attempt {attempt} ({encoder:?} at {} kbit) fits: {actual} bytes <= {target_bytes} byte target",
                    plan.video_kbit
                );
                return Ok(actual);
            }
            retry::NextAction::Retry {
                video_kbit,
                encoder: next_encoder,
            } => {
                crate::log_info!(
                    "attempt {attempt} ({encoder:?} at {} kbit) overshot: {actual} bytes > {target_bytes} byte target; retrying at {video_kbit} kbit on {next_encoder:?}",
                    plan.video_kbit
                );
                plan.video_kbit = video_kbit;
                use_hardware = next_encoder == retry::EncoderKind::Hardware;
            }
            retry::NextAction::GiveUp => {
                crate::log_warn!(
                    "attempt {attempt} ({encoder:?} at {} kbit) overshot: {actual} bytes > {target_bytes} byte target; giving up",
                    plan.video_kbit
                );
                return Err(retry::give_up_error(target_bytes / 1_000_000.0));
            }
        }
    }
    unreachable!("next_action never retries past MAX_RE_ENCODES")
}

/// Runs the post-encode actions per the user's toggles, returning a combined
/// error string for the Done state's `post_error` when any of them fail.
fn run_post_actions(
    inner: &Inner,
    post: &PostActions,
    input: &Path,
    output: &Path,
) -> Option<String> {
    let mut failures: Vec<String> = Vec::new();
    if post.copy_to_clipboard {
        match crate::platform::copy_file_to_clipboard(&inner.app, output) {
            Ok(()) => crate::log_info!("copied {} to clipboard", output.display()),
            Err(e) => {
                crate::log_error!("copy to clipboard failed for {}: {e}", output.display());
                failures.push(format!(
                    "Couldn't copy to clipboard (the file is at {}): {e}",
                    output.to_string_lossy()
                ));
            }
        }
    }
    if post.trash_original {
        match trash::delete(input) {
            Ok(()) => crate::log_info!("moved original {} to Trash", input.display()),
            Err(e) => {
                crate::log_error!("failed to move original {} to Trash: {e}", input.display());
                failures.push(format!("Couldn't move the original to Trash: {e}"));
            }
        }
    }
    (!failures.is_empty()).then(|| failures.join("; "))
}

/// Records a successful (non-reused) encode in the conversion journal.
/// Best-effort: skipped silently when the journal isn't managed (tests).
fn append_journal(
    inner: &Inner,
    job: &QueuedJob,
    output: &Path,
    preset_hash: &str,
    output_bytes: u64,
) {
    let Some(journal) = inner.app.try_state::<crate::journal::Journal>() else {
        return;
    };
    let input_bytes = inner
        .jobs
        .lock()
        .unwrap()
        .iter()
        .find(|j| j.id == job.id)
        .map(|j| j.input_bytes)
        .unwrap_or(0);
    let completed_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    journal.append(crate::journal::ConversionRecord {
        input_path: job.input.to_string_lossy().into_owned(),
        input_bytes,
        output_path: output.to_string_lossy().into_owned(),
        output_bytes,
        preset_hash: preset_hash.to_string(),
        preset_name: job.preset.name.clone(),
        target_mb: job.preset.target_mb,
        completed_at_ms,
    });
}

/// Runs both software encode passes for `plan` — libx264 for mp4, libvpx-vp9
/// for webm. Public so the integration test can exercise the exact pipeline
/// the worker uses (tests can't build an AppHandle).
///
/// `on_progress` receives (pass, overall) with pass 1 mapped to 0..0.5 and
/// pass 2 to 0.5..1.0. `is_cancelled` is checked before each pass and right
/// after spawning, closing the race where cancel() fires between passes.
pub async fn run_passes(
    plan: &plan::EncodePlan,
    info: &probe::ProbeInfo,
    input: &Path,
    passlog_dir: &Path,
    child_slot: &ChildSlot,
    is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    on_progress: &mut (dyn FnMut(u8, f64) + Send),
) -> Result<(), String> {
    if plan.format == OutputFormat::Gif {
        return Err("gif plans are palette-encoded; use run_gif".to_string());
    }
    run_pass(
        1,
        plan,
        info,
        input,
        passlog_dir,
        child_slot,
        is_cancelled,
        on_progress,
    )
    .await?;
    run_pass(
        2,
        plan,
        info,
        input,
        passlog_dir,
        child_slot,
        is_cancelled,
        on_progress,
    )
    .await
}

/// Runs the single-pass h264_videotoolbox encode for `plan`, reporting
/// progress as pass 1 mapped to the FULL 0..1 range. `-allow_sw 1` lets
/// VideoToolbox use Apple's software encoder when no hardware session is
/// available; the worker falls back to the two-pass libx264 path when this
/// errors or leaves a missing/empty output. Public for the integration test.
pub async fn run_hardware_pass(
    plan: &plan::EncodePlan,
    info: &probe::ProbeInfo,
    input: &Path,
    child_slot: &ChildSlot,
    is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    on_progress: &mut (dyn FnMut(u8, f64) + Send),
) -> Result<(), String> {
    let mut cmd = tokio::process::Command::new(bin::ffmpeg_path());
    // `-v error` keeps stderr to actual error lines: without it ffmpeg's
    // input-metadata dump (com.apple.quicktime…) drowns the real failure in
    // the captured tail. `-progress pipe:1` still reports on stdout.
    cmd.args([
        "-y",
        "-v",
        "error",
        "-hide_banner",
        "-nostats",
        "-progress",
        "pipe:1",
        "-i",
    ])
    .arg(input);
    if let Some(vf) = &plan.vf {
        cmd.arg("-vf").arg(vf);
    }
    cmd.args(["-c:v", "h264_videotoolbox"])
        .arg("-b:v")
        .arg(format!("{}k", plan.video_kbit))
        .args(["-allow_sw", "1"]);
    if plan.audio_kbit > 0 {
        cmd.args(["-c:a", "aac"])
            .arg("-b:a")
            .arg(format!("{}k", plan.audio_kbit));
    } else {
        cmd.arg("-an");
    }
    cmd.args(["-movflags", "+faststart"])
        .args(["-f", muxer(plan.format)])
        .arg(&plan.output);

    run_ffmpeg_with_progress(
        "hardware pass",
        cmd,
        info,
        &plan.output,
        child_slot,
        is_cancelled,
        &mut |frac| on_progress(1, frac.clamp(0.0, 1.0)),
    )
    .await
}

/// Width/fps retries allowed after the initial GIF attempt.
const GIF_MAX_RETRIES: u8 = 4;

/// Runs the palette-based GIF engine for `plan`: encode at the planned
/// fps/width, then — bitrate math doesn't exist for GIF — shrink per
/// `plan::gif_retry_params` (width every retry, frame rate too from the
/// third attempt; neither ever raised above the plan's starting params) and
/// re-encode, up to 4 retries. Every attempt writes to the crash-safe
/// `plan::part_path` sibling; a fitting output is verified (enforce_target)
/// and only then renamed onto `plan.output`, while a failure or give-up
/// removes the part file — an oversized or partial gif is never delivered.
/// Returns the final output size, which is always <= `target_bytes`. Each
/// attempt reports progress as pass 1 over the FULL 0..1 range. Public for
/// the integration test.
pub async fn run_gif(
    plan: &plan::EncodePlan,
    info: &probe::ProbeInfo,
    input: &Path,
    target_bytes: f64,
    child_slot: &ChildSlot,
    is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    on_progress: &mut (dyn FnMut(u8, f64) + Send),
) -> Result<u64, String> {
    let params = plan
        .gif
        .ok_or("gif plan is missing its palette parameters")?;
    let mut work = plan.clone();
    work.output = plan::part_path(&plan.output);
    let result = gif_attempts(
        &work,
        info,
        input,
        params,
        target_bytes,
        child_slot,
        is_cancelled,
        on_progress,
    )
    .await;
    match result {
        Ok(actual) => promote_part(&work.output, &plan.output, actual, target_bytes),
        Err(e) => {
            let _ = std::fs::remove_file(&work.output);
            Err(e)
        }
    }
}

/// The encode/shrink loop behind [`run_gif`]; `plan.output` already points
/// at the part sibling and `initial` is the plan's starting params (the
/// retry floors clamp to it). Returns the fitting attempt's size, leaving
/// the file at the part path for the caller to verify and promote.
#[allow(clippy::too_many_arguments)]
async fn gif_attempts(
    plan: &plan::EncodePlan,
    info: &probe::ProbeInfo,
    input: &Path,
    initial: plan::GifParams,
    target_bytes: f64,
    child_slot: &ChildSlot,
    is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    on_progress: &mut (dyn FnMut(u8, f64) + Send),
) -> Result<u64, String> {
    let mut current = initial;
    for attempt in 0..=GIF_MAX_RETRIES {
        run_gif_attempt(
            plan,
            info,
            input,
            current.fps,
            current.max_width,
            child_slot,
            is_cancelled,
            on_progress,
        )
        .await?;
        let actual = std::fs::metadata(&plan.output)
            .map_err(|e| format!("encoded output missing: {e}"))?
            .len();
        if actual as f64 <= target_bytes {
            crate::log_info!(
                "gif attempt {attempt} ({} fps, {} px) fits: {actual} bytes <= {target_bytes} byte target",
                current.fps,
                current.max_width
            );
            return Ok(actual);
        }
        let give_up = if attempt == GIF_MAX_RETRIES {
            true // bounded retries exhausted
        } else {
            let next = plan::gif_retry_params(current, initial, attempt + 1, target_bytes, actual);
            // Unchanged parameters mean every available knob already sits at
            // its floor — re-encoding the exact same settings can't help, so
            // fail early (checked from the FIRST retry on).
            let stuck = next.fps == current.fps && next.max_width == current.max_width;
            if !stuck {
                crate::log_info!(
                    "gif attempt {attempt} ({} fps, {} px) overshot: {actual} bytes > {target_bytes} byte target; retrying at {} fps, {} px",
                    current.fps,
                    current.max_width,
                    next.fps,
                    next.max_width
                );
            }
            current = next;
            stuck
        };
        if give_up {
            crate::log_warn!(
                "gif attempt {attempt} ({} fps, {} px) overshot: {actual} bytes > {target_bytes} byte target; giving up",
                current.fps,
                current.max_width
            );
            return Err(retry::give_up_error(target_bytes / 1_000_000.0));
        }
    }
    unreachable!("the loop returns on the final retry")
}

#[allow(clippy::too_many_arguments)]
async fn run_gif_attempt(
    plan: &plan::EncodePlan,
    info: &probe::ProbeInfo,
    input: &Path,
    fps: u32,
    max_width: u32,
    child_slot: &ChildSlot,
    is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    on_progress: &mut (dyn FnMut(u8, f64) + Send),
) -> Result<(), String> {
    let mut cmd = tokio::process::Command::new(bin::ffmpeg_path());
    // `-v error` keeps stderr to actual error lines: without it ffmpeg's
    // input-metadata dump (com.apple.quicktime…) drowns the real failure in
    // the captured tail. `-progress pipe:1` still reports on stdout.
    cmd.args([
        "-y",
        "-v",
        "error",
        "-hide_banner",
        "-nostats",
        "-progress",
        "pipe:1",
        "-i",
    ])
    .arg(input)
    .arg("-filter_complex")
    .arg(plan::gif_filter(fps, max_width))
    .arg("-an") // GIF never carries audio
    .args(["-f", muxer(plan.format)])
    .arg(&plan.output);

    run_ffmpeg_with_progress(
        "gif encode",
        cmd,
        info,
        &plan.output,
        child_slot,
        is_cancelled,
        &mut |frac| on_progress(1, frac.clamp(0.0, 1.0)),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn run_pass(
    pass: u8,
    plan: &plan::EncodePlan,
    info: &probe::ProbeInfo,
    input: &Path,
    passlog_dir: &Path,
    child_slot: &ChildSlot,
    is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    on_progress: &mut (dyn FnMut(u8, f64) + Send),
) -> Result<(), String> {
    let mut cmd = tokio::process::Command::new(bin::ffmpeg_path());
    // `-v error` keeps stderr to actual error lines: without it ffmpeg's
    // input-metadata dump (com.apple.quicktime…) drowns the real failure in
    // the captured tail. `-progress pipe:1` still reports on stdout.
    cmd.args([
        "-y",
        "-v",
        "error",
        "-hide_banner",
        "-nostats",
        "-progress",
        "pipe:1",
        "-i",
    ])
    .arg(input);
    if let Some(vf) = &plan.vf {
        cmd.arg("-vf").arg(vf);
    }
    match plan.format {
        OutputFormat::Webm => {
            // cpu-used 8 races through the analysis pass; 4 keeps the real
            // encode at a sane quality/speed trade-off.
            cmd.args(["-c:v", "libvpx-vp9"])
                .arg("-b:v")
                .arg(format!("{}k", plan.video_kbit))
                .arg("-pass")
                .arg(pass.to_string())
                .arg("-passlogfile")
                .arg(passlog_dir.join("p"))
                .args(["-row-mt", "1", "-deadline", "good"])
                .args(["-cpu-used", if pass == 1 { "8" } else { "4" }]);
        }
        _ => {
            cmd.args(["-c:v", "libx264", "-preset", "medium"])
                .arg("-b:v")
                .arg(format!("{}k", plan.video_kbit))
                .arg("-pass")
                .arg(pass.to_string())
                .arg("-passlogfile")
                .arg(passlog_dir.join("p"));
        }
    }
    if pass == 1 {
        cmd.args(["-an", "-f", "null", "/dev/null"]);
    } else {
        if plan.audio_kbit > 0 {
            let codec = match plan.format {
                OutputFormat::Webm => "libopus",
                _ => "aac",
            };
            cmd.args(["-c:a", codec])
                .arg("-b:a")
                .arg(format!("{}k", plan.audio_kbit));
        } else {
            cmd.arg("-an");
        }
        if plan.format == OutputFormat::Mp4 {
            cmd.args(["-movflags", "+faststart"]);
        }
        cmd.args(["-f", muxer(plan.format)]).arg(&plan.output);
    }

    run_ffmpeg_with_progress(
        &format!("pass {pass}"),
        cmd,
        info,
        &plan.output,
        child_slot,
        is_cancelled,
        &mut |frac| on_progress(pass, progress::overall(pass, frac)),
    )
    .await
}

/// Lines of the stderr capture surfaced in the job row; the full capture
/// goes to the log file.
const ERROR_TAIL_LINES: usize = 3;
/// Upper bound on the captured stderr lines. With `-v error` the capture is
/// only actual error lines, so this is a safety net against a pathological
/// flood, not a working limit.
const STDERR_CAPTURE_LINES: usize = 500;

/// The last `max_lines` non-empty lines of an ffmpeg stderr capture: the
/// row-visible part of an encode error. Blank lines (and lines of pure
/// whitespace) never count against the budget, so the meaningful error lines
/// at the end of the capture are what the user sees.
pub fn error_tail(stderr: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .collect();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].join("\n")
}

/// The full ffmpeg argv as a shell-pasteable string, for the attempt-start
/// log line (arguments containing spaces are quoted).
fn format_argv(cmd: &tokio::process::Command) -> String {
    let std_cmd = cmd.as_std();
    std::iter::once(std_cmd.get_program())
        .chain(std_cmd.get_args())
        .map(|arg| {
            let arg = arg.to_string_lossy();
            if arg.contains(' ') {
                format!("\"{arg}\"")
            } else {
                arg.into_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Free bytes on the volume holding `path` (POSIX statvfs) — logged when an
/// encode attempt fails, since a full output volume is the classic transient
/// cause that a re-run hours later doesn't reproduce.
#[cfg(target_os = "macos")]
fn free_disk_bytes(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let cpath = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::zeroed();
    // SAFETY: cpath is a valid NUL-terminated path and stat points at
    // properly sized writable memory; statvfs only writes into it.
    if unsafe { libc::statvfs(cpath.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return None;
    }
    let stat = unsafe { stat.assume_init() };
    Some(u64::from(stat.f_bavail).saturating_mul(stat.f_frsize))
}

#[cfg(not(target_os = "macos"))]
fn free_disk_bytes(_path: &Path) -> Option<u64> {
    None
}

/// Best-effort log of the output volume's free space after a failed attempt.
fn log_output_volume_free_space(output: &Path) {
    // The output itself may not exist (or have been cleaned up); its parent
    // directory is on the same volume and always exists.
    let probe_path = output.parent().filter(|p| !p.as_os_str().is_empty());
    let Some(probe_path) = probe_path else { return };
    match free_disk_bytes(probe_path) {
        Some(free) => crate::log_info!(
            "free space on the output volume ({}): {:.1} MB",
            probe_path.display(),
            free as f64 / 1_000_000.0
        ),
        None => crate::log_info!(
            "free space on the output volume ({}) could not be determined",
            probe_path.display()
        ),
    }
}

/// Spawns `cmd`, streams `-progress pipe:1` fractions of the input duration
/// to `on_frac` (always called with 0.0 first and 1.0 on success), and parks
/// the child in `child_slot` so cancel()/shutdown() can kill it. `output` is
/// the attempt's destination, used only for logging (result size on success,
/// the volume's free space on failure).
async fn run_ffmpeg_with_progress(
    label: &str,
    mut cmd: tokio::process::Command,
    info: &probe::ProbeInfo,
    output: &Path,
    child_slot: &ChildSlot,
    is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    on_frac: &mut (dyn FnMut(f64) + Send),
) -> Result<(), String> {
    if is_cancelled() {
        return Err("cancelled".to_string());
    }

    crate::log_info!("ffmpeg {label} starting: {}", format_argv(&cmd));

    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to start ffmpeg: {e}"))?;
    let stdout = child.stdout.take().ok_or("ffmpeg stdout unavailable")?;
    let stderr = child.stderr.take().ok_or("ffmpeg stderr unavailable")?;
    *child_slot.lock().unwrap() = Some(child);
    if is_cancelled() {
        // cancel() may have fired after the slot was empty; kill what we just spawned.
        if let Some(child) = child_slot.lock().unwrap().as_mut() {
            let _ = child.start_kill();
        }
    }

    let stderr_tail = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut tail: VecDeque<String> = VecDeque::new();
        while let Ok(Some(line)) = lines.next_line().await {
            if tail.len() >= STDERR_CAPTURE_LINES {
                tail.pop_front();
            }
            tail.push_back(line);
        }
        tail.into_iter().collect::<Vec<_>>().join("\n")
    });

    on_frac(0.0);
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if let Some(secs) = progress::parse_progress_line(&line) {
            let frac = if info.duration_secs > 0.0 {
                secs / info.duration_secs
            } else {
                0.0
            };
            on_frac(frac);
        }
    }

    let child = child_slot.lock().unwrap().take();
    let status = match child {
        Some(mut child) => child
            .wait()
            .await
            .map_err(|e| format!("ffmpeg wait failed: {e}"))?,
        None => return Err("ffmpeg process disappeared".to_string()),
    };
    let tail = stderr_tail.await.unwrap_or_default();

    if !status.success() {
        // The FULL capture goes to the log; the row gets only the meaningful
        // last lines, with the "ffmpeg {label} failed (...)" prefix intact.
        if tail.trim().is_empty() {
            crate::log_error!("ffmpeg {label} failed ({status}); no stderr output captured");
        } else {
            crate::log_error!("ffmpeg {label} failed ({status}); full stderr:\n{tail}");
        }
        log_output_volume_free_space(output);
        let row_tail = error_tail(&tail, ERROR_TAIL_LINES);
        return Err(if row_tail.is_empty() {
            format!("ffmpeg {label} failed ({status})")
        } else {
            format!("ffmpeg {label} failed ({status})\n{row_tail}")
        });
    }
    let out_bytes = std::fs::metadata(output).map(|m| m.len()).ok();
    crate::log_info!(
        "ffmpeg {label} finished ({status}); output size: {}",
        // Pass 1 writes to /dev/null, so there is nothing to stat.
        out_bytes.map_or_else(|| "n/a".to_string(), |b| format!("{b} bytes"))
    );
    on_frac(1.0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_reuse_serves_an_existing_under_target_output() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("clip (tamped 823f).mp4");
        std::fs::write(&out, vec![0u8; 900]).unwrap();
        assert_eq!(check_reuse(&out, 1000.0), Some(900));
        assert!(out.exists(), "an under-target output must be kept");
        // Exactly at the target still reuses — the guarantee is <=.
        std::fs::write(&out, vec![0u8; 1000]).unwrap();
        assert_eq!(check_reuse(&out, 1000.0), Some(1000));
    }

    #[test]
    fn check_reuse_deletes_a_stale_over_target_output() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("clip (tamped 823f).mp4");
        std::fs::write(&out, vec![0u8; 1001]).unwrap();
        // Over target: never re-serve it — delete tamp's own stale artifact
        // so the fresh encode can claim the base output path.
        assert_eq!(check_reuse(&out, 1000.0), None);
        assert!(
            !out.exists(),
            "the stale over-target output must be deleted"
        );
    }

    #[test]
    fn check_reuse_passes_through_when_nothing_exists() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("clip (tamped 823f).mp4");
        assert_eq!(check_reuse(&out, 1000.0), None);
    }

    #[test]
    fn check_reuse_deletes_a_zero_byte_output_instead_of_serving_it() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("clip (tamped 823f).mp4");
        std::fs::write(&out, b"").unwrap();
        // A zero-byte file is a crash leftover, never a deliverable output.
        assert_eq!(check_reuse(&out, 1000.0), None);
        assert!(!out.exists(), "the zero-byte leftover must be deleted");
    }

    #[test]
    fn check_reuse_leaves_a_directory_alone() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("clip (tamped 823f).mp4");
        std::fs::create_dir(&out).unwrap();
        // Not a regular file: never reuse, never delete — the fresh encode
        // goes to a numbered sibling instead.
        assert_eq!(check_reuse(&out, 1000.0), None);
        assert!(out.is_dir(), "a squatting directory must be left alone");
    }

    #[test]
    fn check_reuse_leaves_a_symlink_alone() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.mp4");
        std::fs::write(&real, vec![0u8; 900]).unwrap();
        let out = dir.path().join("clip (tamped 823f).mp4");
        std::os::unix::fs::symlink(&real, &out).unwrap();
        // Even when the link target would fit: a symlink is not tamp's own
        // hash-named artifact, so neither serve nor delete it.
        assert_eq!(check_reuse(&out, 1000.0), None);
        assert!(
            std::fs::symlink_metadata(&out).unwrap().is_symlink(),
            "the symlink must be left alone"
        );
        assert!(real.exists());
    }

    #[test]
    fn check_reuse_sweeps_over_target_numbered_siblings() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("clip (tamped 823f).mp4");
        std::fs::write(&base, vec![0u8; 1001]).unwrap();
        let over = dir.path().join("clip (tamped 823f 2).mp4");
        std::fs::write(&over, vec![0u8; 1500]).unwrap();
        let under = dir.path().join("clip (tamped 823f 3).mp4");
        std::fs::write(&under, vec![0u8; 500]).unwrap();
        let other_hash = dir.path().join("clip (tamped ffff 2).mp4");
        std::fs::write(&other_hash, vec![0u8; 1500]).unwrap();
        let part = dir.path().join("clip (tamped 823f 4).mp4.part");
        std::fs::write(&part, vec![0u8; 1500]).unwrap();

        assert_eq!(check_reuse(&base, 1000.0), None);
        assert!(!base.exists(), "the over-target base must be deleted");
        assert!(!over.exists(), "the over-target sibling must be swept");
        assert!(under.exists(), "an under-target sibling must survive");
        assert!(other_hash.exists(), "other configurations must survive");
        assert!(part.exists(), "in-flight part files must survive the sweep");
    }

    #[test]
    fn error_tail_surfaces_the_error_lines_after_metadata_noise() {
        // The user-reported failure shape: ffmpeg dumps the input metadata
        // (com.apple.quicktime…) before the actual error lines; the row must
        // show the errors, not the noise.
        let stderr = "Input #0, mov,mp4,m4a,3gp,3g2,mj2, from 'rec.mov':\n\
                      \x20 Metadata:\n\
                      \x20   com.apple.quicktime.make: Apple\n\
                      \x20   com.apple.quicktime.model: MacBookPro18,3\n\
                      \x20   com.apple.quicktime.software: macOS 15.1\n\
                      \n\
                      [libx264 @ 0x7f8] could not open the encoder\n\
                      Error while opening encoder for output stream #0:0\n\
                      Conversion failed!";
        assert_eq!(
            error_tail(stderr, 3),
            "[libx264 @ 0x7f8] could not open the encoder\n\
             Error while opening encoder for output stream #0:0\n\
             Conversion failed!"
        );
    }

    #[test]
    fn error_tail_skips_blank_and_whitespace_only_lines() {
        let stderr = "first real line\n\n   \n\t\nlast real line\n\n";
        assert_eq!(error_tail(stderr, 3), "first real line\nlast real line");
    }

    #[test]
    fn error_tail_returns_everything_when_shorter_than_the_cap() {
        assert_eq!(error_tail("only line", 3), "only line");
        assert_eq!(error_tail("", 3), "");
        assert_eq!(error_tail("\n \n", 3), "");
    }

    #[test]
    fn error_tail_trims_trailing_whitespace_per_line() {
        let stderr = "a   \nb\t\nc";
        assert_eq!(error_tail(stderr, 2), "b\nc");
    }

    #[test]
    fn check_reuse_sweep_runs_when_a_non_file_squats_on_the_base() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("clip (tamped 823f).mp4");
        std::fs::create_dir(&base).unwrap();
        let over = dir.path().join("clip (tamped 823f 2).mp4");
        std::fs::write(&over, vec![0u8; 1500]).unwrap();

        assert_eq!(check_reuse(&base, 1000.0), None);
        assert!(base.is_dir(), "the squatting directory must be left alone");
        assert!(!over.exists(), "the over-target sibling must be swept");
    }
}
