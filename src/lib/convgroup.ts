import type { ConversionRecord } from "./ipc";

/** One output of a conversion: a single's lone output, or one split part. */
export interface ConvPart {
  path: string;
  bytes: number;
}

export type ConvNode =
  | {
      kind: "single"; inputPath: string; inputBytes: number;
      presetName: string; inputCreatedMs: number; completedAtMs: number;
      output: ConvPart;
    }
  | {
      kind: "group"; folder: string; inputPath: string; inputBytes: number;
      presetName: string; inputCreatedMs: number; completedAtMs: number;
      totalBytes: number; parts: ConvPart[];
    };

/** The directory holding a path; "" when there is no separator. */
function parentDir(p: string): string {
  const i = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
  return i < 0 ? "" : p.slice(0, i);
}

/** Flat journal records → singles + multi-part groups, newest-first.
 *  Structure comes straight from the record: one output is a single, N outputs
 *  is a split group. (The old `(tamped …)` filename heuristic is gone — the
 *  journal now stores the shape explicitly.) */
export function groupConversions(records: ConversionRecord[]): ConvNode[] {
  const nodes: ConvNode[] = [];
  for (const r of records) {
    if (r.outputs.length > 1) {
      nodes.push({
        kind: "group",
        folder: parentDir(r.outputs[0].path),
        inputPath: r.inputPath,
        inputBytes: r.inputBytes,
        presetName: r.presetName,
        inputCreatedMs: r.inputCreatedMs,
        completedAtMs: r.completedAtMs,
        totalBytes: r.outputs.reduce((s, o) => s + o.bytes, 0),
        parts: r.outputs.map((o) => ({ path: o.path, bytes: o.bytes })),
      });
    } else {
      const o = r.outputs[0];
      nodes.push({
        kind: "single",
        inputPath: r.inputPath,
        inputBytes: r.inputBytes,
        presetName: r.presetName,
        inputCreatedMs: r.inputCreatedMs,
        completedAtMs: r.completedAtMs,
        output: { path: o.path, bytes: o.bytes },
      });
    }
  }
  return nodes.sort((a, b) => b.completedAtMs - a.completedAtMs);
}
