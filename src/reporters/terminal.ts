import type { Finding } from "../rules/index.ts";

export interface TerminalReporter {
  report(findings: readonly Finding[]): void;
}
