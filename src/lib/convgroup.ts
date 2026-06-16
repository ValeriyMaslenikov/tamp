import type { ConversionRecord } from "./ipc";

export type ConvNode =
  | { kind: "single"; rec: ConversionRecord; completedAtMs: number }
  | {
      kind: "group"; folder: string; inputPath: string; inputBytes: number;
      presetName: string; inputCreatedMs: number; completedAtMs: number;
      totalBytes: number; parts: ConversionRecord[];
    };

// keep in lockstep with src-tauri/src/journal.rs (parent_dir/basename/is_part_path)
function parentDir(p: string): string {
  const i = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
  return i < 0 ? "" : p.slice(0, i);
}
function basename(p: string): string {
  return p.split(/[\\/]/).pop() ?? p;
}
/** A directory whose name ends in "(tamped …)" — a split output folder. */
const TAMPED_DIR = /\(tamped .+\)$/;
/** A FILE that is itself a "(tamped …)" output, e.g.
 *  `clip (tamped Discord 10MB 9ca1).mp4` — a standalone conversion. */
const TAMPED_FILE = /\(tamped [^)]+\)\.[^.]+$/;

/** A split *part* is a bare part file (`name N.ext`) inside a "(tamped …)"
 *  output folder. A standalone output that merely sits inside a tamped folder —
 *  e.g. re-compressing a part — carries the "(tamped …)" suffix in its OWN name
 *  and is its own conversion (rendered as a single), not a part of the original.
 *  Decided purely from the stored output path: the journal is immutable history,
 *  not a live read of the folder. */
export function isPartPath(outputPath: string): boolean {
  return (
    TAMPED_DIR.test(parentDir(outputPath)) && !TAMPED_FILE.test(basename(outputPath))
  );
}

/** Flat journal records → singles + multi-part groups, newest-first. */
export function groupConversions(records: ConversionRecord[]): ConvNode[] {
  const groups = new Map<string, ConversionRecord[]>();
  const nodes: ConvNode[] = [];
  for (const r of records) {
    if (isPartPath(r.outputPath)) {
      const key = parentDir(r.outputPath);
      (groups.get(key) ?? groups.set(key, []).get(key)!).push(r);
    } else {
      nodes.push({ kind: "single", rec: r, completedAtMs: r.completedAtMs });
    }
  }
  for (const [folder, parts] of groups) {
    parts.sort((a, b) => a.outputPath.localeCompare(b.outputPath, undefined, { numeric: true }));
    const completedAtMs = Math.max(...parts.map((p) => p.completedAtMs));
    nodes.push({
      kind: "group", folder, inputPath: parts[0].inputPath, inputBytes: parts[0].inputBytes,
      presetName: parts[0].presetName, inputCreatedMs: parts[0].inputCreatedMs,
      completedAtMs, totalBytes: parts.reduce((s, p) => s + p.outputBytes, 0), parts,
    });
  }
  return nodes.sort((a, b) => b.completedAtMs - a.completedAtMs);
}
