// WebdriverIO config for the tauri-driver smoke suite — drives the REAL built
// app end-to-end via WebDriver, not the mocked-IPC browser harness in ../e2e.
//
// WINDOWS-ONLY. tauri-driver is the WebDriver proxy: it spawns the native
// WebDriver for the platform's webview (on Windows that's Microsoft Edge
// WebDriver, msedgedriver, which must match the installed WebView2 runtime
// major version) and forwards the W3C session to it, launching our app binary
// in between. macOS uses WKWebView, which has no stable WebDriver story, so
// this suite is gated to Windows in CI.
//
// The app binary is the RELEASE build produced by `bun tauri build`
// (target/release/tamp.exe). The CI job builds it before invoking wdio. We
// launch it with TAMP_E2E=1 so lib.rs shows + pins the panel (otherwise the
// release-only hide-on-blur handler closes the window the moment WebDriver's
// automation takes focus — see apply_e2e_mode in src-tauri/src/lib.rs).
//
// NOTE (CI-authoritative): this config + the specs are validated by careful
// reading on the ARM64 dev VM; a real run needs windows-latest with an x64
// release build and a matching msedgedriver, so the CI job is the source of
// truth. Building/running locally on this VM is not expected to work.
import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

// ESM-safe __dirname (this package is "type": "module"). Repo root is the
// parent of this e2e-native/ dir.
const HERE = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = join(HERE, "..");

// The release binary `bun tauri build` emits. productName is "tamp"
// (src-tauri/tauri.conf.json), so the Windows binary is tamp.exe.
const APP_BINARY = join(REPO_ROOT, "src-tauri", "target", "release", "tamp.exe");

// tauri-driver listens here and proxies to the native WebDriver. Default port
// is 4444; we pin it so the WDIO `hostname`/`port` below match.
const TAURI_DRIVER_PORT = 4444;

let tauriDriver: ChildProcess | undefined;

export const config: WebdriverIO.Config = {
  runner: "local",
  specs: ["./specs/**/*.spec.ts"],
  maxInstances: 1, // a single native window — never run smoke specs in parallel
  hostname: "127.0.0.1",
  port: TAURI_DRIVER_PORT,
  // tauri-driver is the only "browser" here; the WebView2 session is described
  // by the tauri:options capability (the app binary + its launch env).
  capabilities: [
    {
      // @ts-expect-error tauri:options is a tauri-driver vendor capability not
      // in the upstream WebdriverIO Capabilities type.
      "tauri:options": {
        application: APP_BINARY,
        // Show + pin the panel so the WebView2 window stays attachable.
        env: { TAMP_E2E: "1" },
      },
      "ms:edgeOptions": {},
    },
  ],
  logLevel: "info",
  // The release build can take a moment to spawn the WebView2 host; be generous
  // but bounded so a hung launch fails the job rather than hanging CI.
  connectionRetryTimeout: 120_000,
  connectionRetryCount: 3,
  waitforTimeout: 15_000,
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: {
    ui: "bdd",
    timeout: 90_000,
  },
  // TS loading: WebdriverIO v9 auto-detects `tsx` (a devDep here) and uses it to
  // run this .ts config + the .ts specs directly, so no build/transpile step or
  // tsconfig wiring is needed. (The v7-era `autoCompileOpts` was removed in v9.)

  // Spin tauri-driver up before the session and tear it down after — wdio's
  // service hook model. Keeping it inline (vs. the wdio-tauri service) avoids an
  // extra dep and makes the proxy wiring explicit.
  onPrepare: () => {
    // Surface a clear error in CI if the build step was skipped.
    if (!existsSync(APP_BINARY)) {
      throw new Error(
        `App binary not found at ${APP_BINARY}. Run \`bun tauri build\` (or the ` +
          `debug build) before the WDIO smoke suite.`,
      );
    }
  },
  beforeSession: () => {
    // `cargo install tauri-driver --locked` puts tauri-driver on PATH (the
    // .cargo/bin dir, which the CI job adds to PATH). It needs msedgedriver
    // reachable too; pass --native-driver if it isn't on PATH.
    const nativeDriver = process.env.TAURI_NATIVE_DRIVER;
    const args = [`--port=${TAURI_DRIVER_PORT}`];
    if (nativeDriver) {
      args.push(`--native-driver=${nativeDriver}`);
    }
    tauriDriver = spawn("tauri-driver", args, {
      stdio: [null, process.stdout, process.stderr],
    });
    tauriDriver.on("error", (err) => {
      console.error("tauri-driver failed to start:", err);
    });
  },
  afterSession: () => {
    tauriDriver?.kill();
    // Best-effort sweep of any orphaned msedgedriver tauri-driver spawned, so a
    // crashed session doesn't leave the port held for the next CI step.
    if (process.platform === "win32") {
      spawnSync("taskkill", ["/F", "/IM", "msedgedriver.exe"], { stdio: "ignore" });
    }
  },
};
