export { detectProject } from "./detect.ts";

import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { findingRecord, findingRecordWidth } from "./types.ts";
import type { FindingRecord, InspectOptions, ScanResult } from "./types.ts";

export type {
  FindingRecord, InspectOptions, PerformanceProfile, RuleMetadata, ScanLanguage, ScanResult, ScanSeverity,
} from "./types.ts";
export { findingRecord, findingRecordWidth };

const require = createRequire(import.meta.url);
const defaultRulePaths = [
  "../../rules/rust/excessive-clones.yaml",
  "../../rules/go/excessive-goroutines.yaml",
].map((path) => fileURLToPath(new URL(path, import.meta.url)));

interface NativeEngine {
  inspect(request: string): Promise<{ metadata: string; findings: Uint32Array }>;
}

/** Load rule files and scan in Rust. No AST nodes or per-rule callbacks cross this boundary. */
export async function inspect(options: InspectOptions): Promise<ScanResult> {
  if (options.threshold !== undefined &&
      (!Number.isInteger(options.threshold) || options.threshold < 0 || options.threshold > 0xffffffff)) {
    throw new TypeError("threshold must be an unsigned 32-bit integer");
  }
  for (const [name, value] of [["maxFindings", options.maxFindings]] as const) {
    if (value !== undefined && (!Number.isInteger(value) || value < 0 || value > 0xffffffff)) {
      throw new TypeError(`${name} must be an unsigned 32-bit integer`);
    }
  }
  let native: NativeEngine;
  try {
    native = require("../../native/build/badbox.node") as NativeEngine;
  } catch (cause) {
    throw new Error("Cannot load Badbox native engine. In a source checkout, run bun run build:native.", { cause });
  }
  const response = await native.inspect(JSON.stringify({
    paths: options.paths,
    rulePaths: options.rulePaths ?? defaultRulePaths,
    threshold: options.threshold,
    profile: options.profile ?? false,
    maxFindings: options.maxFindings,
  }));
  const decodeStarted = performance.now();
  const metadata = JSON.parse(response.metadata) as Omit<ScanResult, "findings">;
  const result: ScanResult = { ...metadata, findings: response.findings };
  const decodeMs = performance.now() - decodeStarted;
  if (result.performance) {
    (result.performance as { jsDecodeMs: number }).jsDecodeMs = decodeMs;
  }
  return result;
}

/** Iterate compact findings without materializing a second result array. */
export function* iterateFindings(result: ScanResult): IterableIterator<FindingRecord> {
  if (result.recordWidth !== findingRecordWidth || result.findings.length % result.recordWidth !== 0) {
    throw new Error("invalid native finding record layout");
  }
  for (let offset = 0; offset < result.findings.length; offset += result.recordWidth) {
    const fileId = result.findings[offset + findingRecord.fileId]!;
    const ruleId = result.findings[offset + findingRecord.ruleId]!;
    const file = result.files[fileId];
    const rule = result.rules[ruleId];
    if (file === undefined || rule === undefined) throw new Error("invalid native finding table index");
    yield {
      fileId,
      ruleId,
      file,
      rule,
      ownerStart: result.findings[offset + findingRecord.ownerStart]!,
      ownerEnd: result.findings[offset + findingRecord.ownerEnd]!,
      observed: result.findings[offset + findingRecord.observed]!,
    };
  }
}

export interface ScanOptions {
  readonly cwd: string;
}

export interface ScanSummary {
  readonly scannedFiles: number;
  readonly findingCount: number;
}
