import type { ConversionRecord } from "./ipc";

export type ConvNode =
  | { kind: "single"; rec: ConversionRecord; completedAtMs: number }
  | {
      kind: "group"; folder: string; inputPath: string; inputBytes: number;
      presetName: string; inputCreatedMs: number; completedAtMs: number;
      totalBytes: number; parts: ConversionRecord[];
    };

function parentDir(p: string): string {
  const i = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
  return i < 0 ? "" : p.slice(0, i);
}
const TAMPED = /\(tamped .+\)$/;
/** A split part lives in a "(tamped …)" output folder; a single output sits
 *  directly in its source folder. */
export function isPartPath(outputPath: string): boolean {
  return TAMPED.test(parentDir(outputPath));
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
