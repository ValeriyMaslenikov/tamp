//! Integration coverage for the process-lifetime probe cache, driven through
//! the injectable cache core `tamp_lib::encoder::probe::probe_cached_with`.
//!
//! Reachable without invoking ffmpeg: `probe_cached_with` takes the probe fn as
//! an argument, so the test injects a call-counting stub and asserts that a
//! second `probe_cached_with` for the SAME key returns the cached value without
//! re-invoking the probe fn. (The public `probe_cached` would spawn the bundled
//! ffprobe; the injectable core is the pure path, so this needs no ffmpeg —
//! mirrors the in-crate unit test, here over the public API.)
//!
//! The cache is keyed on the file's path|mtime|size and is process-global, so
//! each test writes a UNIQUE on-disk file to avoid key collisions with the
//! other tests sharing the same process cache.

use std::sync::atomic::{AtomicUsize, Ordering};

use tamp_lib::encoder::probe::{probe_cached_with, ProbeInfo};

fn sample_info(duration: f64) -> ProbeInfo {
    ProbeInfo {
        duration_secs: duration,
        width: 1920,
        height: 1080,
        fps: 30.0,
        has_audio: true,
    }
}

#[tokio::test]
async fn second_probe_for_the_same_key_hits_the_cache_without_reprobing() {
    // A unique file so the path|mtime|size key cannot collide with the other
    // probe-cache tests sharing this process's global cache.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("integration-clip-a.mp4");
    std::fs::write(&path, b"integration sample a").unwrap();

    let calls = AtomicUsize::new(0);

    // First lookup: a miss runs the injected probe fn and stores the result.
    let first = probe_cached_with(&path, || {
        calls.fetch_add(1, Ordering::SeqCst);
        async { Ok(sample_info(12.0)) }
    })
    .await
    .expect("first probe must succeed");
    assert_eq!(first.duration_secs, 12.0);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // Second lookup for the same key: a HIT — the cached value is returned and
    // the probe fn is NOT invoked again (a different value here would prove a
    // re-probe; we must still see 12.0 and one call total).
    let second = probe_cached_with(&path, || {
        calls.fetch_add(1, Ordering::SeqCst);
        async { Ok(sample_info(99.0)) }
    })
    .await
    .expect("cached lookup must succeed");
    assert_eq!(
        second.duration_secs, 12.0,
        "the cached value must be returned, not the stub's new value"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "a cache hit must not re-invoke the probe fn"
    );
}

#[tokio::test]
async fn a_failed_probe_is_not_cached_and_re_probes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("integration-clip-b.mp4");
    std::fs::write(&path, b"integration sample b").unwrap();

    let calls = AtomicUsize::new(0);

    // A failing probe must NOT be memoized.
    let first = probe_cached_with(&path, || {
        calls.fetch_add(1, Ordering::SeqCst);
        async { Err::<ProbeInfo, _>("transient ffprobe failure".to_string()) }
    })
    .await;
    assert!(first.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // The next lookup for the same key re-invokes the probe fn (the Err was not
    // cached) and this time succeeds, populating the cache.
    let second = probe_cached_with(&path, || {
        calls.fetch_add(1, Ordering::SeqCst);
        async { Ok(sample_info(7.0)) }
    })
    .await
    .expect("a retried probe must succeed");
    assert_eq!(second.duration_secs, 7.0);
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    // The successful probe IS cached now: a third lookup is a hit (no re-probe).
    let third = probe_cached_with(&path, || {
        calls.fetch_add(1, Ordering::SeqCst);
        async { Ok(sample_info(123.0)) }
    })
    .await
    .expect("cached lookup must succeed");
    assert_eq!(third.duration_secs, 7.0);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
