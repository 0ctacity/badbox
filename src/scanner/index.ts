export { detectProject } from "./detect.ts";

import { findingRecord, findingRecordWidth } from "./types.ts";
import type { CheckResult, FindingRecord, InspectOptions } from "./types.ts";
import { loadNativeEngine } from "./native.ts";

export type {
  CheckLanguage, CheckResult, CheckSeverity, FindingRecord, InspectOptions, PerformanceProfile, RuleMetadata,
} from "./types.ts";
export { findingRecord, findingRecordWidth };

/** Load rule files and check source in Rust. No AST nodes or per-rule callbacks cross this boundary. */
export async function inspect(options: InspectOptions): Promise<CheckResult> {
  if (!options.rulePaths.length) throw new TypeError("rulePaths must not be empty");
  if (options.threshold !== undefined &&
      (!Number.isInteger(options.threshold) || options.threshold < 0 || options.threshold > 0xffffffff)) {
    throw new TypeError("threshold must be an unsigned 32-bit integer");
  }
  for (const [name, value] of [["maxFindings", options.maxFindings]] as const) {
    if (value !== undefined && (!Number.isInteger(value) || value < 0 || value > 0xffffffff)) {
      throw new TypeError(`${name} must be an unsigned 32-bit integer`);
    }
  }
  for (const [key, value] of Object.entries(options.parameters ?? {})) {
    if (!key.length) throw new TypeError("parameter key must not be empty");
    if (typeof value === "number" &&
        (!Number.isInteger(value) || value < 0 || value > 0xffffffff)) {
      throw new TypeError(`parameter ${key} must be an unsigned 32-bit integer, boolean, or string`);
    }
  }
  const native = loadNativeEngine();
  const response = await native.inspect(JSON.stringify({
    paths: options.paths,
    rulePaths: options.rulePaths,
    parameters: options.parameters ?? {},
    threshold: options.threshold,
    profile: options.profile ?? false,
    maxFindings: options.maxFindings,
  }));
  const decodeStarted = performance.now();
  const metadata = JSON.parse(response.metadata) as Omit<CheckResult, "findings">;
  const result: CheckResult = { ...metadata, findings: response.findings };
  const decodeMs = performance.now() - decodeStarted;
  if (result.performance) {
    (result.performance as { jsDecodeMs: number }).jsDecodeMs = decodeMs;
  }
  return result;
}

/** Iterate compact findings without materializing a second result array. */
export function* iterateFindings(result: CheckResult): IterableIterator<FindingRecord> {
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

export interface CheckOptions {
  readonly cwd: string;
}

export interface CheckSummary {
  readonly checkedFiles: number;
  readonly findingCount: number;
}
