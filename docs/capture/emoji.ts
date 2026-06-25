// The demo cast for README/wiki captures — one source of truth for both the
// generated thumbnails (gen-thumbs.ts) and the fixture data (demo-data.ts).
// Names are intentionally playful and reuse the originals from the old panel.
export interface CastMember {
  slug: string;
  file: string;
  emoji: string;
  grad: [string, string]; // CSS gradient stops for the thumbnail card
  sizeBytes: number;
  durationSecs: number;
  recordedAgoMs: number; // how long before "now" it was recorded
}

const MIN = 60_000;
const HOUR = 3_600_000;

// Sizes/durations are kept internally believable: capture bitrate ~50 Mbps
// (high-DPI screen/game capture), so size ≈ 6.25 MB/s × duration. The two clips
// the demos actually convert (boss → the main GIF, duck → the split GIF) are
// short enough that landing under Discord's 10 MB stays a watchable bitrate
// (~1.6 Mbps), so "260 MB → 9.4 MB" reads as plausible, not magic.
export const CAST: CastMember[] = [
  { slug: "cat",    file: "cat-knocks-over-everything.mov", emoji: "🐈", grad: ["#f0823c", "#e35d6a"], sizeBytes: 150_000_000, durationSecs: 24,  recordedAgoMs: 3 * MIN },
  { slug: "boss",   file: "ranked-match-final-boss.mov",    emoji: "🎮", grad: ["#6d5cf0", "#3a2f88"], sizeBytes: 260_000_000, durationSecs: 42,  recordedAgoMs: 21 * MIN },
  { slug: "duck",   file: "duck-debugging-session.mov",     emoji: "🦆", grad: ["#1aa6a6", "#147a7a"], sizeBytes: 610_000_000, durationSecs: 98,  recordedAgoMs: 2 * HOUR },
  { slug: "deploy", file: "deploy-friday-5pm.mov",          emoji: "🚀", grad: ["#7c5cfc", "#4a36a8"], sizeBytes: 300_000_000, durationSecs: 48,  recordedAgoMs: 19 * HOUR },
  { slug: "demo",   file: "demo-went-fine-honestly.mov",    emoji: "⭐", grad: ["#f0a93c", "#e3743c"], sizeBytes: 175_000_000, durationSecs: 28,  recordedAgoMs: 22 * HOUR },
  { slug: "pizza",  file: "pizza-tracker-speedrun.mov",     emoji: "🍕", grad: ["#e3563c", "#a8362f"], sizeBytes: 132_000_000, durationSecs: 21,  recordedAgoMs: 26 * HOUR },
];
