import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { friendlyError } from "./errors";

// friendlyError logs the raw value locally (console.error) for diagnosis; silence
// it in tests and assert it's the raw value, never the friendly copy.
beforeEach(() => {
  vi.spyOn(console, "error").mockImplementation(() => {});
});
afterEach(() => {
  vi.restoreAllMocks();
});

const GENERIC = "Something went wrong. Check the logs (tray → Open Logs) for details.";

describe("friendlyError", () => {
  it("maps each known backend fragment to its friendly copy", () => {
    // Real fragments from commands.rs / encoder/hw.rs / encoder/mod.rs / shortcuts.rs.
    expect(friendlyError("recents scan failed: io error")).toBe(
      "Couldn't refresh your recordings. Try again.",
    );
    expect(friendlyError("clipboard task failed: JoinError(...)")).toBe(
      "Couldn't copy the file. Try again, or reveal it in your file manager.",
    );
    expect(friendlyError("Couldn't copy to clipboard (Downloads): denied")).toBe(
      "Couldn't copy the file. Try again, or reveal it in your file manager.",
    );
    expect(friendlyError("couldn't open /Users/x/clip.mp4: No such file")).toBe(
      "Couldn't open the file — it may have been moved or deleted.",
    );
    expect(friendlyError("couldn't reveal /Users/x/clip.mp4: No such file")).toBe(
      "Couldn't show the file in your file manager.",
    );
    expect(friendlyError("File no longer exists at /Users/x/clip.mp4")).toBe(
      "That file is no longer there — it may have been moved or deleted.",
    );
    expect(friendlyError("ffmpeg pass1 failed (exit status: 1)")).toBe(
      "The video tool hit an error. Check the logs (tray → Open Logs) for details.",
    );
    expect(friendlyError("ffmpeg -encoders exited with exit status: 69")).toBe(
      "The video tool hit an error. Check the logs (tray → Open Logs) for details.",
    );
    expect(friendlyError("cannot probe ffmpeg encoders (broken)")).toBe(
      "The video tool hit an error. Check the logs (tray → Open Logs) for details.",
    );
    expect(friendlyError("failed to clear global shortcuts: registration error")).toBe(
      "Couldn't update the keyboard shortcut.",
    );
    expect(friendlyError('Invalid "compress latest" shortcut "Ctrl+": bad accelerator')).toBe(
      "Couldn't update the keyboard shortcut.",
    );
  });

  it("prefers the more specific 'no longer exists' over reveal/open wording", () => {
    // A reveal of a deleted output rejects with "File no longer exists at …",
    // not "couldn't reveal …", and must read as the moved/deleted message.
    expect(friendlyError("File no longer exists at /tmp/out.mp4")).toBe(
      "That file is no longer there — it may have been moved or deleted.",
    );
  });

  it("falls back to the generic message for an unknown string", () => {
    expect(friendlyError("some unexpected backend explosion")).toBe(GENERIC);
    expect(friendlyError("")).toBe(GENERIC);
  });

  it("matches case-insensitively", () => {
    expect(friendlyError("CLIPBOARD TASK FAILED: ...")).toBe(
      "Couldn't copy the file. Try again, or reveal it in your file manager.",
    );
    expect(friendlyError("FFMPEG PASS1 FAILED")).toBe(
      "The video tool hit an error. Check the logs (tray → Open Logs) for details.",
    );
    expect(friendlyError("Recents Scan Failed")).toBe(
      "Couldn't refresh your recordings. Try again.",
    );
  });

  it("coerces non-string values and never throws", () => {
    // Tauri usually rejects with a string, but guard against Error/objects too.
    expect(friendlyError(new Error("couldn't open file: x"))).toBe(
      "Couldn't open the file — it may have been moved or deleted.",
    );
    expect(friendlyError(undefined)).toBe(GENERIC);
    expect(friendlyError(null)).toBe(GENERIC);
    expect(friendlyError({ code: 42 })).toBe(GENERIC);
  });

  it("logs the raw value locally and keeps it out of the returned copy", () => {
    const spy = vi.spyOn(console, "error").mockImplementation(() => {});
    const raw = "couldn't open /secret/path.mp4: ENOENT";
    const copy = friendlyError(raw);
    expect(spy).toHaveBeenCalledWith(raw); // raw stays local (console only)
    expect(copy).not.toContain("/secret/path.mp4"); // never leaked to the toast
  });
});
