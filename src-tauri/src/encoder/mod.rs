pub mod bin;
pub mod plan;
pub mod probe;
pub mod progress;

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

// Re-exported so integration tests can build presets even though the
// `settings` module itself is private to the crate.
pub use crate::settings::Preset;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobState {
    pub id: String,
    pub input_path: String,
    pub input_name: String,
    pub output_path: Option<String>,
    pub preset_id: String,
    pub phase: Phase,
    pub progress: f64, // 0..1 overall
    pub input_bytes: u64,
    pub output_bytes: Option<u64>,
    pub error: Option<String>,
    /// Set when a post-action (clipboard copy / trash original) fails after a
    /// successful encode; `phase` stays `Done`.
    pub post_error: Option<String>,
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
            phase: Phase::Queued,
            progress: 0.0,
            input_bytes: meta.len(),
            output_bytes: None,
            error: None,
            post_error: None,
        };
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
            })
            .is_err()
        {
            self.inner.pending.fetch_sub(1, Ordering::SeqCst);
            return Err("encoder worker is not running".to_string());
        }
        Ok(id)
    }

    pub fn cancel(&self, id: &str) {
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
        eprintln!("tamp: failed to emit encode:state: {e}");
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
        if let Some(state) = update_job(inner, &job.id, |j| j.phase = Phase::Cancelled) {
            emit_state(inner, &state, true);
        }
        if inner.pending.load(Ordering::SeqCst) == 0 {
            crate::tray::set_progress(&inner.app, None);
        }
        return;
    }

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
        }
        if !was_cancelled {
            eprintln!("tamp: job {} failed: {err}", job.id);
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
    let info = probe::probe(&job.input).await?;
    let mut plan = plan::build_plan(&info, &job.preset, &job.input)?;
    let tmp = tempfile::tempdir().map_err(|e| format!("cannot create temp dir: {e}"))?;
    let target_bytes = job.preset.target_mb * 1_000_000.0;

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
    let is_cancelled = move || c_inner.cancelled.lock().unwrap().contains(&c_id);

    // One bitrate-adjustment retry if the first attempt overshoots the target.
    for attempt in 0..2u8 {
        run_passes(
            &plan,
            &info,
            &job.input,
            tmp.path(),
            &inner.child_slot,
            &is_cancelled,
            &mut on_progress,
        )
        .await?;

        if let Some(state) = update_job(inner, &job.id, |j| {
            j.phase = Phase::Verifying;
            j.progress = 1.0;
        }) {
            emit_state(inner, &state, true);
        }

        let actual = std::fs::metadata(&plan.output)
            .map_err(|e| format!("encoded output missing: {e}"))?
            .len();

        if actual as f64 <= target_bytes || attempt == 1 {
            // Run post-actions first so their failures ride along on the
            // final Done state instead of vanishing into stderr.
            let mut post_failures: Vec<String> = Vec::new();
            if job.post.copy_to_clipboard {
                if let Err(e) = crate::platform::copy_file_to_clipboard(&inner.app, &plan.output) {
                    eprintln!("tamp: copy to clipboard failed: {e}");
                    post_failures.push(format!(
                        "Couldn't copy to clipboard (the file is at {}): {e}",
                        plan.output.to_string_lossy()
                    ));
                }
            }
            if job.post.trash_original {
                if let Err(e) = trash::delete(&job.input) {
                    eprintln!("tamp: failed to move original to Trash: {e}");
                    post_failures.push(format!("Couldn't move the original to Trash: {e}"));
                }
            }
            let post_error = (!post_failures.is_empty()).then(|| post_failures.join("; "));
            if let Some(state) = update_job(inner, &job.id, |j| {
                j.phase = Phase::Done;
                j.progress = 1.0;
                j.output_bytes = Some(actual);
                j.post_error = post_error;
            }) {
                emit_state(inner, &state, true);
            }
            return Ok(());
        }

        let adjusted = (plan.video_kbit as f64 * (target_bytes / actual as f64) * 0.97) as u32;
        plan.video_kbit = adjusted.max(100);
        if let Some(state) = update_job(inner, &job.id, |j| {
            j.phase = Phase::Pass1;
            j.progress = 0.0;
            j.output_bytes = None;
        }) {
            emit_state(inner, &state, true);
        }
    }
    unreachable!("retry loop always returns on the second attempt")
}

/// Runs both libx264 passes for `plan`. Public so the integration test can
/// exercise the exact pipeline the worker uses (tests can't build an AppHandle).
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
    if is_cancelled() {
        return Err("cancelled".to_string());
    }

    let mut cmd = tokio::process::Command::new(bin::ffmpeg_path());
    cmd.args([
        "-y",
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
    cmd.args(["-c:v", "libx264", "-preset", "medium"])
        .arg("-b:v")
        .arg(format!("{}k", plan.video_kbit))
        .arg("-pass")
        .arg(pass.to_string())
        .arg("-passlogfile")
        .arg(passlog_dir.join("p"));
    if pass == 1 {
        cmd.args(["-an", "-f", "null", "/dev/null"]);
    } else {
        if plan.audio_kbit > 0 {
            cmd.args(["-c:a", "aac"])
                .arg("-b:a")
                .arg(format!("{}k", plan.audio_kbit));
        } else {
            cmd.arg("-an");
        }
        cmd.args(["-movflags", "+faststart"]).arg(&plan.output);
    }
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
            if tail.len() >= 30 {
                tail.pop_front();
            }
            tail.push_back(line);
        }
        tail.into_iter().collect::<Vec<_>>().join("\n")
    });

    on_progress(pass, progress::overall(pass, 0.0));
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if let Some(secs) = progress::parse_progress_line(&line) {
            let frac = if info.duration_secs > 0.0 {
                secs / info.duration_secs
            } else {
                0.0
            };
            on_progress(pass, progress::overall(pass, frac));
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
        return Err(format!("ffmpeg pass {pass} failed ({status})\n{tail}"));
    }
    on_progress(pass, progress::overall(pass, 1.0));
    Ok(())
}
