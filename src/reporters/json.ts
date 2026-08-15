import type { Finding } from "../rules/index.ts";

export interface JsonReporter {
  report(findings: readonly Finding[]): string;
}
