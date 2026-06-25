# e2e-native — tauri-driver smoke suite

End-to-end smoke tests that drive the **real built app** over WebDriver, the
top layer of the test pyramid above the unit tests (`src/**/*.test.ts`,
`src-tauri`) and the mocked-IPC UI E2E suite (`../e2e`, Playwright).

## What it does

`tauri-driver` is a WebDriver proxy: it spawns the platform's native WebDriver
(on **Windows**, Microsoft Edge WebDriver — `msedgedriver`, which must match the
installed **WebView2** runtime major version) and launches our app binary
between them, exposing the app's WebView as a W3C session WebdriverIO can drive.

The app is launched with **`TAMP_E2E=1`**, which makes `src-tauri/src/lib.rs`
(`apply_e2e_mode`) show **and pin** the panel. Without the pin, the release-only
hide-on-blur handler closes the panel the moment WebDriver's automation window
takes focus, leaving nothing to attach to. The env var is a strict no-op on
normal runs.

The specs assert only that the app boots and the panel chrome renders (the three
tabs, Videos active by default) — a wiring smoke, deliberately minimal.

## Platform

**Windows only.** macOS uses WKWebView, which has no stable WebDriver story.
The local dev VM here is ARM64; a real run needs an **x64 release build** plus a
matching x64 `msedgedriver`, so **CI (`windows-latest`) is the authoritative
runner** — see the `e2e-native` job in `.github/workflows/ci.yml`. Building and
running locally on the ARM64 VM is best-effort and not expected to work.

## Running (on a Windows x64 machine / in CI)

```sh
# 1. Build the app (the smoke drives target/release/tamp.exe)
bun install --frozen-lockfile
bun scripts/fetch-ffmpeg.ts
bun tauri build

# 2. Install the WebDriver proxy + the native driver
cargo install tauri-driver --locked
#   msedgedriver must match the installed WebView2 runtime (CI installs it via
#   the msedgedriver action / a pinned download); put it on PATH or pass
#   TAURI_NATIVE_DRIVER=/path/to/msedgedriver.exe

# 3. Install + run the suite
cd e2e-native
bun install
bun run test
```

Override the WebDriver port with the constant in `wdio.conf.ts`; point at a
specific msedgedriver with `TAURI_NATIVE_DRIVER`.
