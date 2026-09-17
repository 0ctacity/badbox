import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import { iterateFindings, type CheckResult, type InspectOptions, type PerformanceProfile } from "../src/scanner/index.ts";

export type Language = "rust" | "go";
export type Density = "sparse" | "dense";
type Selector = { pattern: string } | { kind: string };

const selectors: Record<Language, Record<Density, readonly Selector[]>> = {
  rust: {
    sparse: ["clone", "unwrap", "is_some", "is_none", "ok", "to_owned", "to_string", "is_empty", "len"]
      .map((method) => ({ pattern: `$VALUE.${method}()` }))
      .concat([{ pattern: "$VALUE.expect($$$ARGS)" }]),
    dense: ["call_expression", "identifier", "field_identifier", "field_expression", "arguments",
      "let_declaration", "return_expression", "reference_expression", "type_identifier", "block"]
      .map((kind) => ({ kind })),
  },
  go: {
    sparse: [{ kind: "go_statement" }, ...["panic($$$ARGS)", "close($$$ARGS)", "recover()",
      "copy($$$ARGS)", "delete($$$ARGS)", "make($$$ARGS)", "append($$$ARGS)", "new($TYPE)", "len($VALUE)"]
      .map((pattern) => ({ pattern }))],
    dense: ["call_expression", "identifier", "argument_list", "selector_expression", "short_var_declaration",
      "assignment_statement", "return_statement", "binary_expression", "expression_statement", "block"]
      .map((kind) => ({ kind })),
  },
};

/** Benchmark-only selectors; these are not product rules or engineering recommendations. */
export function makeRules(language: Language, density: Density, count: number) {
  if (!Number.isInteger(count) || count < 1 || count > 100) throw new Error("rule count must be 1..100");
  return Array.from({ length: count }, (_, index) => ({
    version: 1,
    id: `bench/${language}-${density}-${String(index).padStart(3, "0")}`,
    language,
    summary: "Benchmark selection, not a code-quality recommendation",
    severity: "info",
    select: selectors[language][density][index % 10]!,
    owner: { nearest: language === "rust"
      ? ["function_item", "closure_expression"]
      : ["function_declaration", "method_declaration", "func_literal"] },
    aggregate: "count",
    threshold: { gt: 0 },
    evidence: { subject: "selected syntax sites" },
  }));
}

/** Hash every compact owner/count record. Rule IDs are map keys, not part of each digest. */
export function fingerprints(result: CheckResult): Record<string, string> {
  if (result.diagnostics.length) throw new Error(`benchmark scan has diagnostics: ${JSON.stringify(result.diagnostics)}`);
  if (result.truncated) throw new Error("benchmark scan unexpectedly truncated full-output validation");
  const hashes = new Map<string, ReturnType<typeof createHash>>();
  for (const finding of iterateFindings(result)) {
    const id = finding.rule.id;
    let hash = hashes.get(id);
    if (!hash) { hash = createHash("sha256"); hashes.set(id, hash); }
    hash.update(JSON.stringify([
      finding.file, finding.ownerStart, finding.ownerEnd, finding.observed,
    ])).update("\n");
  }
  return Object.fromEntries([...hashes.entries()].sort(([a], [b]) => a.localeCompare(b))
    .map(([id, hash]) => [id, hash.digest("hex")]));
}

export function summarize(values: readonly number[]) {
  if (!values.length || values.some((value) => !Number.isFinite(value))) throw new Error("expected finite samples");
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return { min: sorted[0]!, median: sorted.length % 2 ? sorted[middle]!
    : (sorted[middle - 1]! + sorted[middle]!) / 2, max: sorted.at(-1)! };
}

export interface WorkerConfig {
  options: InspectOptions;
  expected: Record<string, string>;
  checkedFiles: number;
  warmups: number;
  samples: number;
  profile?: boolean;
}

export interface WorkerResult {
  scanMs: number[];
  warmupMs: number[];
  findings: number;
  findingBytes: number;
  processWallMs: number;
  peakRssBytes: number;
  cpuTimeMicroseconds: number;
  performance?: PerformanceProfile;
}

export function workerTimeoutMs(config: Pick<WorkerConfig, "samples" | "warmups">): number {
  if (!Number.isInteger(config.samples) || config.samples < 1 ||
      !Number.isInteger(config.warmups) || config.warmups < 0) throw new Error("invalid worker sample counts");
  return 60_000 * (config.samples + config.warmups);
}

export async function measureWorker(configPath: string): Promise<WorkerResult> {
  const timeoutMs = workerTimeoutMs(await Bun.file(configPath).json());
  const started = performance.now();
  const worker = fileURLToPath(new URL("./worker.ts", import.meta.url));
  const process = Bun.spawn([globalThis.process.execPath, worker, configPath], {
    stdout: "pipe", stderr: "pipe",
  });
  const timeout = setTimeout(() => process.kill(), timeoutMs);
  try {
    const [stdout, stderr, code] = await Promise.all([
      new Response(process.stdout).text(), new Response(process.stderr).text(), process.exited,
    ]);
    const processWallMs = performance.now() - started;
    if (code !== 0) throw new Error(`benchmark worker failed (${code}): ${stderr}`);
    const usage = process.resourceUsage();
    if (!usage || usage.maxRSS <= 0) throw new Error("peak RSS is unavailable on this runtime");
    return { ...JSON.parse(stdout), processWallMs, peakRssBytes: Number(usage.maxRSS), cpuTimeMicroseconds: Number(usage.cpuTime.total) };
  } finally {
    clearTimeout(timeout);
  }
}
