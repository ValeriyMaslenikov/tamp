// Canned data for the mocked IPC harness. Mirrors src/lib/ipc.ts (the
// camelCase shapes the backend serializes). One place to keep the boot data
// realistic; specs override individual commands via the fixture's setMock /
// seedSettings instead of editing this.
import type { Settings, ConversionRecord, RecentVideo } from "../src/lib/ipc";

/**
 * A complete default Settings the boot path renders against. `locale:"en"`
 * pins the UI to English so assertions are deterministic; `onboardingSeen:true`
 * keeps the first-run notice from covering the panel in the generic harness
 * (the onboarding spec seeds `false` explicitly). `updateCheckEnabled:false`
 * so no update modal appears unless a spec opts in.
 */
export function defaultSettings(): Settings {
  return {
    watchedFolders: ["/Users/e2e/Movies"],
    copyToClipboard: true,
    trashOriginal: false,
    useHardwareEncoder: true,
    presets: [
      {
        id: "discord-10mb",
        name: "Discord (10MB)",
        targetMb: 10,
        maxFps: null,
        maxWidth: null,
        scalePercent: null,
        stripAudio: false,
        format: "mp4",
        split: { mode: "off", by: "parts", parts: 2, seconds: 30 },
      },
      {
        id: "slack-25mb",
        name: "Slack (25MB)",
        targetMb: 25,
        maxFps: null,
        maxWidth: null,
        scalePercent: null,
        stripAudio: false,
        format: "mp4",
        split: { mode: "off", by: "parts", parts: 2, seconds: 30 },
      },
    ],
    defaultPresetId: "discord-10mb",
    launchAtLogin: false,
    shortcutCompressLatest: "CmdOrCtrl+Alt+T",
    shortcutTogglePanel: "CmdOrCtrl+Alt+O",
    staleWarnMinutes: 10,
    openAfterConvert: "off",
    videosLayout: "quick-pick",
    theme: "system",
    contextMenuEnabled: true,
    recentsLimit: 50,
    onboardingSeen: true,
    updateCheckEnabled: false,
    lastDismissedUpdateVersion: null,
    locale: "en",
  };
}

/** A couple of recent videos for the Videos-tab list state. */
export function sampleRecents(): RecentVideo[] {
  return [
    {
      path: "/Users/e2e/Movies/screen-recording-1.mov",
      name: "screen-recording-1.mov",
      sizeBytes: 52_428_800,
      createdMs: Date.now() - 60_000,
      thumbPath: null,
      isOutput: false,
      conversion: null,
      durationSecs: null,
    },
    {
      path: "/Users/e2e/Movies/demo-clip.mp4",
      name: "demo-clip.mp4",
      sizeBytes: 18_874_368,
      createdMs: Date.now() - 3_600_000,
      thumbPath: null,
      isOutput: false,
      conversion: null,
      durationSecs: null,
    },
  ];
}

/** A split conversion (one multi-output record) plus a single, for Converted. */
export function sampleConversions(): ConversionRecord[] {
  return [
    {
      inputPath: "/Users/e2e/Movies/long-recording.mov",
      inputBytes: 120_000_000,
      outputs: [
        { path: "/Users/e2e/Movies/long-recording (tamped 1of2).mp4", bytes: 9_800_000 },
        { path: "/Users/e2e/Movies/long-recording (tamped 2of2).mp4", bytes: 9_600_000 },
      ],
      presetHash: "hash-split",
      presetName: "Discord (10MB)",
      targetMb: 10,
      completedAtMs: Date.now() - 120_000,
      inputCreatedMs: Date.now() - 600_000,
    },
    {
      inputPath: "/Users/e2e/Movies/short-clip.mov",
      inputBytes: 30_000_000,
      outputs: [
        { path: "/Users/e2e/Movies/short-clip (tamped).mp4", bytes: 8_200_000 },
      ],
      presetHash: "hash-single",
      presetName: "Slack (25MB)",
      targetMb: 25,
      completedAtMs: Date.now() - 240_000,
      inputCreatedMs: Date.now() - 900_000,
    },
  ];
}
