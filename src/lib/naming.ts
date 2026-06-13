// Output-file naming helpers — mirror the backend's "(tamped …)" suffix
// pattern (src-tauri encoder/plan.rs). Unit tested in naming.test.ts.

/**
 * Suffix the encoder appends to compressed outputs, matched at the end of the
 * file stem. Forms: " (tamped a3f2)" / " (tamped a3f2 2)" (hashed), the named
 * " (tamped Discord a3f2)" / " (tamped Discord 10MB a3f2 2)" (one or more
 * name words before the hash), legacy split parts " (tamped a3f2 p2of5)", and
 * the legacy " (tamped)" / " (tamped 2)". The hash is 4 lowercase hex chars
 * and always sits immediately before any collision counter / part token
 * (never both); any words ahead of it are the cosmetic preset name.
 */
const OUTPUT_SUFFIX =
  / \(tamped(?:(?: [^ )]+)* [0-9a-f]{4}(?: \d+| p\d+of\d+)?| \d+| p\d+of\d+)?\)$/;

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
