import type { Rule } from "./types.ts";

export type {
  Finding,
  FindingEvidence,
  Rule,
  Severity,
  SourceLocation,
} from "./types.ts";

export const builtInRules: readonly Rule[] = [];
