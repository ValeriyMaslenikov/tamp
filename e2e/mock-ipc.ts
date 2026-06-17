// Tauri IPC mock for Playwright UI E2E.
//
// The crux of the harness: the real frontend (src/main.ts) talks to the Rust
// backend exclusively through `@tauri-apps/api`, which routes every call —
// invoke(), the event plugin's listen()/unlisten()/emit(), getCurrentWindow(),
// getCurrentWebview(), getVersion(), convertFileSrc() — through the global
// `window.__TAURI_INTERNALS__`. In a plain browser that global is absent and the
// app throws on the first invoke().
//
// We hand-roll that global (rather than importing `@tauri-apps/api/mocks`
// mockIPC) so the whole shim is a single self-contained function with NO module
// imports: Playwright's `page.addInitScript(fn)` serializes the function source
// and runs it in the page BEFORE any module script — i.e. before main.ts boots —
// which is exactly the ordering we need and which an ESM import could not give
// us without a bundling step. The shim is deterministic, override-driven, and
// reads its command/event responses from `window.__E2E__` so a test can change
// any response before the app loads (see fixtures.ts: setMock / emit).
//
// installTauriMock() below is `page.addInitScript`-ed as a function reference,
// so it must stay dependency-free (everything it needs is inline). The shapes
// it returns mirror src/lib/ipc.ts.

/** A command response: a static value, or a function of the invoke args. */
export type MockResponse =
  | unknown
  | ((args: Record<string, unknown>) => unknown);

/** The bridge the harness plants on `window` so a spec can drive the mock. */
export interface E2EBridge {
  /** Per-command overrides, keyed by the Tauri command name. */
  mocks: Record<string, MockResponse>;
  /** Registered event listeners: event name -> set of callback ids. */
  listeners: Record<string, number[]>;
  /** The default settings object the boot path reads (overridable per test). */
  settings: Record<string, unknown> | null;
  /** Emit a Tauri event to every live listener (used by drawer/encode specs). */
  emit: (event: string, payload: unknown) => void;
  /** Commands the app actually invoked, in order (for assertions). */
  calls: { cmd: string; args: Record<string, unknown> }[];
}

declare global {
  interface Window {
    __E2E__?: E2EBridge;
  }
}

/**
 * The init function injected (as serialized source) BEFORE the app boots. It is
 * intentionally self-contained — no imports, no references to module-scope
 * symbols — because Playwright runs it via `addInitScript(fn)` in a fresh page
 * context. `defaultSettings` is passed in so the canned data lives in one place
 * (mock-ipc.ts) but is still inlined into the page.
 */
export function installTauriMock(defaultSettings: Record<string, unknown>): void {
  // `window` is typed as the DOM Window; the Tauri globals aren't on it. A loose
  // alias lets the shim read/assign __E2E__ + the Tauri globals without colliding
  // with @tauri-apps/api's own ambient declarations. Declared INSIDE the function
  // because addInitScript serializes only this function's source — a module-scope
  // reference would be undefined in the page.
  const w = window as unknown as Record<string, unknown>;

  // The shared bridge: a test may have pre-seeded window.__E2E__ via an earlier
  // addInitScript (setMock/seedSettings); merge onto it so those win.
  const seeded = (w.__E2E__ as Partial<E2EBridge> | undefined) ?? undefined;

  // A few flows persist via save_settings and then hard-reload the webview (the
  // language switch does `location.reload()` so the app re-reads settings in the
  // new locale). A reload tears down window.__E2E__ — including the recorded
  // calls and the saved settings — which would lose the very write a spec wants
  // to assert. So the bridge mirrors calls + settings into sessionStorage (it
  // survives a same-origin reload but not a new context) and restores them on
  // re-init. addInitScript runs this BEFORE the seeded overrides re-apply, so a
  // post-reload boot picks up the persisted (just-saved) settings rather than
  // snapping back to the seed. Wrapped in try/catch so a storage-less context
  // degrades to the in-memory bridge.
  const CALLS_KEY = "__E2E_calls__";
  const SETTINGS_KEY = "__E2E_settings__";
  function readStored<T>(key: string): T | undefined {
    try {
      const raw = window.sessionStorage.getItem(key);
      return raw == null ? undefined : (JSON.parse(raw) as T);
    } catch {
      return undefined;
    }
  }
  function writeStored(key: string, value: unknown): void {
    try {
      window.sessionStorage.setItem(key, JSON.stringify(value));
    } catch {
      /* storage unavailable — in-memory only */
    }
  }

  const storedCalls = readStored<E2EBridge["calls"]>(CALLS_KEY);
  const storedSettings = readStored<Record<string, unknown>>(SETTINGS_KEY);

  const bridge: E2EBridge = {
    mocks: (seeded && seeded.mocks) || {},
    listeners: {},
    // Precedence for the boot settings: a value persisted by an earlier
    // save_settings (a reload happened) wins, so the reloaded app sees the
    // just-saved state; then a per-test seed; then the canned default.
    settings:
      storedSettings !== undefined
        ? storedSettings
        : seeded && seeded.settings !== undefined
          ? seeded.settings
          : defaultSettings,
    calls: storedCalls ?? [],
    emit: () => {},
  };

  // callback registry — transformCallback hands the backend an id; emit() and
  // the event plugin run the stored callbacks by id.
  const callbacks = new Map<number, (data: unknown) => unknown>();
  let nextId = 1;

  function transformCallback(cb: (data: unknown) => unknown, once = false): number {
    const id = nextId++;
    callbacks.set(id, (data: unknown) => {
      if (once) callbacks.delete(id);
      return cb && cb(data);
    });
    return id;
  }

  // Deliver a payload to one listener id in the shape @tauri-apps/api/event
  // expects: { event, id, payload }.
  function runListener(event: string, id: number, payload: unknown): void {
    const cb = callbacks.get(id);
    if (cb) cb({ event, id, payload });
  }

  bridge.emit = (event: string, payload: unknown): void => {
    const ids = bridge.listeners[event] || [];
    for (const id of ids.slice()) runListener(event, id, payload);
  };

  // The default response table for every command the UI calls on boot (and the
  // few it calls on common interactions). A per-command override in
  // bridge.mocks always wins; `get_settings`/`save_settings` read/write
  // bridge.settings so a settings change round-trips through the mock.
  function defaultFor(cmd: string, args: Record<string, unknown>): unknown {
    switch (cmd) {
      // ----- boot path -----
      case "os_info":
        return "macos";
      case "get_settings":
        return bridge.settings;
      case "save_settings": {
        // Echo back (and persist) the settings the UI saved, like the backend.
        const next = (args.settings as Record<string, unknown>) ?? bridge.settings;
        bridge.settings = next;
        // Mirror to sessionStorage so a save-then-reload flow (language switch)
        // boots into the just-saved state, not the original seed.
        writeStored(SETTINGS_KEY, next);
        return next;
      }
      case "list_recents":
        return [];
      case "list_conversions":
        return [];
      case "queue_state":
        return [];
      case "unreachable_folders":
        return [];
      case "check_for_update":
        return null;
      case "notification_permission":
      case "request_notification_permission":
        return "granted";
      // ----- lazy per-row resolves -----
      case "recent_thumb":
      case "conversion_thumb":
        return null;
      case "recent_duration":
        return null;
      case "ensure_preview":
        return "";
      // ----- actions (resolve to a benign value) -----
      case "enqueue":
      case "custom_convert":
        return "job-mock-1";
      case "pick_videos":
        return [];
      case "pick_folder":
        return null;
      // void commands
      case "set_pin":
      case "set_context_menu":
      case "cancel_job":
      case "reveal":
      case "copy_file":
      case "copy_files":
      case "open_file":
      case "open_url":
      case "open_notification_settings":
        return null;
      default:
        return null;
    }
  }

  function resolveMock(cmd: string, args: Record<string, unknown>): unknown {
    const override = bridge.mocks[cmd];
    if (override !== undefined) {
      return typeof override === "function"
        ? (override as (a: Record<string, unknown>) => unknown)(args)
        : override;
    }
    return defaultFor(cmd, args);
  }

  // The single entry point @tauri-apps/api/core routes every invoke through, and
  // the event/window/webview/app plugins route their plugin commands through too
  // (cmd like "plugin:event|listen", "plugin:app|version", "plugin:window|hide").
  async function invoke(
    cmd: string,
    args?: Record<string, unknown>,
  ): Promise<unknown> {
    const a = args || {};

    // Event plugin: listen/unlisten/emit (drives onPanelShown/onEncodeState/
    // onSettingsChanged and getCurrentWebview().onDragDropEvent()).
    if (cmd === "plugin:event|listen") {
      const event = a.event as string;
      const handler = a.handler as number; // a transformCallback id
      (bridge.listeners[event] ||= []).push(handler);
      return handler;
    }
    if (cmd === "plugin:event|unlisten") {
      const event = a.event as string;
      const id = a.eventId as number;
      const arr = bridge.listeners[event];
      if (arr) {
        const i = arr.indexOf(id);
        if (i !== -1) arr.splice(i, 1);
      }
      return null;
    }
    if (cmd === "plugin:event|emit" || cmd === "plugin:event|emit_to") {
      return null; // the frontend never emits; nothing to deliver
    }

    // App plugin: getVersion() in preferences.ts.
    if (cmd === "plugin:app|version") return "0.0.0-e2e";
    if (cmd === "plugin:app|name") return "tamp";
    if (cmd === "plugin:app|tauri_version") return "2.0.0";

    // Window plugin: getCurrentWindow().hide() (Esc) and friends — accept all.
    if (cmd.startsWith("plugin:window|")) return null;
    if (cmd.startsWith("plugin:webview|")) return null;
    if (cmd.startsWith("plugin:")) return null;

    // A real app command: record it (mirroring to sessionStorage so a reload
    // doesn't lose the call a spec wants to assert) and resolve from the mock.
    bridge.calls.push({ cmd, args: a });
    writeStored(CALLS_KEY, bridge.calls);
    return resolveMock(cmd, a);
  }

  // convertFileSrc(): the app turns local paths into asset URLs for <img>/<video>
  // srcs. A stub keeps those elements from erroring on a missing global.
  function convertFileSrc(filePath: string, protocol = "asset"): string {
    return `https://${protocol}.localhost/${encodeURIComponent(filePath)}`;
  }

  w.__TAURI_INTERNALS__ = {
    invoke,
    transformCallback,
    unregisterCallback: (id: number) => callbacks.delete(id),
    convertFileSrc,
    // getCurrentWindow()/getCurrentWebview() read this metadata synchronously.
    metadata: {
      currentWindow: { label: "main" },
      currentWebview: { windowLabel: "main", label: "main" },
    },
  };
  w.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener: (_event: string, id: number) => callbacks.delete(id),
  };

  w.__E2E__ = bridge;
}
