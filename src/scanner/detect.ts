export type DetectedProject = "react" | "typescript" | "unknown";

/**
 * Reserved for deterministic project detection based on repository evidence.
 */
export function detectProject(): DetectedProject {
  return "unknown";
}
