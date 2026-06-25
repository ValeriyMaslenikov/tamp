// Curated demo fixtures for captures, derived from the cast in emoji.ts. The IPC
// mock answers list_recents/recent_thumb/recent_duration from these so the panel
// renders the same coherent set across every shot.
import type { RecentVideo, ConversionRecord, Settings } from "../../src/lib/ipc";
import { CAST } from "./emoji";

const DIR = "/Users/demo/Movies";
// Anchored to the real current time at capture so the rows read "3m ago",
// "21m ago", etc. (a fixed past constant would render as "a year ago"). The
// captures are regenerated on demand, so absolute determinism isn't needed.
const NOW = Date.now();

function pathFor(file: string): string {
  return `${DIR}/${file}`;
}

export const THUMB_BY_PATH: Record<string, string> = Object.fromEntries(
  CAST.map((m) => [pathFor(m.file), m.slug]),
);

export function demoRecents(): RecentVideo[] {
  return CAST.map((m) => ({
    path: pathFor(m.file),
    name: m.file,
    sizeBytes: m.sizeBytes,
    createdMs: NOW - m.recordedAgoMs,
    // thumbPath stays null: the list resolves thumbs lazily via recent_thumb,
    // which the mock overrides (see harness.ts) to return a path the route
    // handler serves. Setting it here would skip the lazy path we rely on.
    thumbPath: null,
    isOutput: false,
    conversion: null,
    durationSecs: m.durationSecs,
  }));
}

export function demoConversions(): ConversionRecord[] {
  // One split (group) + one single, mirroring e2e/canned.ts's shape so the
  // Converted tab renders a realistic tree if we ever shoot it.
  const cat = CAST[0];
  const duck = CAST[2];
  return [
    {
      inputPath: pathFor(duck.file),
      inputBytes: duck.sizeBytes,
      outputs: [
        { path: `${DIR}/duck-debugging-session (tamped 1of2).mp4`, bytes: 9_800_000 },
        { path: `${DIR}/duck-debugging-session (tamped 2of2).mp4`, bytes: 9_600_000 },
      ],
      presetHash: "hash-split",
      presetName: "Discord (10MB)",
      targetMb: 10,
      completedAtMs: NOW - 120_000,
      inputCreatedMs: NOW - 600_000,
    },
    {
      inputPath: pathFor(cat.file),
      inputBytes: cat.sizeBytes,
      outputs: [{ path: `${DIR}/cat-knocks-over-everything (tamped).mp4`, bytes: 8_200_000 }],
      presetHash: "hash-single",
      presetName: "Slack (25MB)",
      targetMb: 25,
      completedAtMs: NOW - 240_000,
      inputCreatedMs: NOW - 900_000,
    },
  ];
}

export function demoSettings(layout: "quick-pick" | "active-bar"): Partial<Settings> {
  return { videosLayout: layout, theme: "dark", locale: "en", onboardingSeen: true };
}
