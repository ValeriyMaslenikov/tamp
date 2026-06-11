// Output-file naming helpers — mirror the backend's "(tamped …)" suffix
// pattern (src-tauri encoder/plan.rs). Unit tested in naming.test.ts.

/**
 * Suffix the encoder appends to compressed outputs, matched at the end of the
 * file stem: " (tamped a3f2)" / " (tamped a3f2 2)" (hashed) and the legacy
 * " (tamped)" / " (tamped 2)" forms. The hash is 4 lowercase hex chars.
 */
const OUTPUT_SUFFIX = / \(tamped(?: [0-9a-f]{4})?(?: \d+)?\)$/;

/**
 * Derived original stem of an output file name:
 * "clip (tamped a3f2 2).mp4" -> "clip". Names without the suffix only lose
 * their extension.
 */
export function stripOutputSuffix(name: string): string {
  const dot = name.lastIndexOf(".");
  const stem = dot > 0 ? name.slice(0, dot) : name;
  return stem.replace(OUTPUT_SUFFIX, "");
}
