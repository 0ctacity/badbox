import { expect, test } from "bun:test";
import { fileURLToPath } from "node:url";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { inspect } from "../src/scanner/index.ts";
import { makeRules, fingerprints, measureWorker, summarize, workerTimeoutMs } from "../benchmarks/harness.ts";
import { runBenchmark } from "../benchmarks/run.ts";

test("scaling packs have exact relevant rule counts and varied selectors", () => {
  for (const language of ["rust", "go"] as const) {
    for (const density of ["sparse", "dense"] as const) {
      for (const count of [1, 10, 50, 100]) {
        const rules = makeRules(language, density, count);
        expect(rules).toHaveLength(count);
        expect(new Set(rules.map((rule) => rule.id)).size).toBe(count);
        expect(rules.every((rule) => rule.language === language)).toBe(true);
        expect(new Set(rules.map((rule) => JSON.stringify(rule.select))).size).toBe(Math.min(10, count));
      }
    }
  }
  expect(() => makeRules("rust", "dense", 0)).toThrow();
  expect(summarize([4, 1, 3, 2, 5])).toEqual({ min: 1, median: 3, max: 5 });
  expect(() => summarize([])).toThrow();
  expect(workerTimeoutMs({ samples: 1, warmups: 0 })).toBe(60_000);
  expect(workerTimeoutMs({ samples: 5, warmups: 1 })).toBe(360_000);
  expect(() => workerTimeoutMs({ samples: 0, warmups: 0 })).toThrow();
});

test("benchmark workers consume native results and detect incorrect expectations", async () => {
  const root = await mkdtemp(join(tmpdir(), "badbox-bench-test-"));
  try {
    const paths = [fileURLToPath(new URL("./fixtures/counts/sample.rs", import.meta.url))];
    const rulePaths = [];
    for (const [index, rule] of makeRules("rust", "dense", 10).entries()) {
      const path = join(root, `${index}.yaml`);
      await Bun.write(path, JSON.stringify(rule));
      rulePaths.push(path);
    }
    const options = { paths, rulePaths };
    const reference = await inspect(options);
    const expected = fingerprints(reference);
    const config = join(root, "worker.json");
    await Bun.write(config, JSON.stringify({ options, expected, checkedFiles: 1, warmups: 1, samples: 2, profile: true }));
    const result = await measureWorker(config);
    expect(result.scanMs).toHaveLength(2);
    expect(result.warmupMs).toHaveLength(1);
    expect(result.scanMs.every((time) => time > 0)).toBe(true);
    expect(result.peakRssBytes).toBeGreaterThan(1_000_000);
    expect(result.findings).toBe(reference.findingCount);
    const performance = result.performance;
    expect(performance).toBeDefined();
    if (!performance) throw new Error("profiled benchmark worker omitted performance data");
    expect(performance.resultCacheHits).toBe(1);
    expect(performance.resultCacheMisses).toBe(0);
    expect(performance.selectorExecutions).toBe(0);
    expect(performance.ruleEvaluations).toBe(0);
    expect(performance.ruleLoadMs).toBeGreaterThanOrEqual(0);
    expect(performance.serializationMs).toBeGreaterThanOrEqual(0);
    expect(performance.jsDecodeMs).toBeGreaterThanOrEqual(0);
    await Bun.write(config, JSON.stringify({ options, expected, checkedFiles: 1, warmups: 0, samples: 1 }));
    const unprofiled = await measureWorker(config);
    expect((unprofiled as { performance?: unknown }).performance).toBeUndefined();
    await Bun.write(config, JSON.stringify({ options, expected: {}, checkedFiles: 1, warmups: 0, samples: 1 }));
    await expect(measureWorker(config)).rejects.toThrow("fingerprint");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("all benchmark selector templates compile and batched results agree with individual rules", async () => {
  const root = await mkdtemp(join(tmpdir(), "badbox-bench-selectors-"));
  try {
    for (const language of ["rust", "go"] as const) {
      for (const density of ["sparse", "dense"] as const) {
        const extension = language === "rust" ? "rs" : "go";
        const paths = [fileURLToPath(new URL(`./fixtures/counts/sample.${extension}`, import.meta.url))];
        const rulePaths = [];
        const expected: Record<string, string> = {};
        for (const [index, rule] of makeRules(language, density, 10).entries()) {
          const path = join(root, `${language}-${density}-${index}.yaml`);
          await Bun.write(path, JSON.stringify(rule));
          rulePaths.push(path);
          Object.assign(expected, fingerprints(await inspect({ paths, rulePaths: [path] })));
        }
        expect(fingerprints(await inspect({ paths, rulePaths }))).toEqual(expected);
      }
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("benchmark report records input hashes, sample arrays, and memory without a Zova checkout", async () => {
  const root = await mkdtemp(join(tmpdir(), "badbox-bench-report-"));
  try {
    const output = join(root, "report.json");
    await runBenchmark({ output, samples: 1, ruleCounts: [1, 10], progress: () => {} });
    const report = await Bun.file(output).json();
    expect(report.status).toBe("complete");
    expect(report.rows).toHaveLength(8);
    expect(report.inputs).toHaveLength(2);
    expect(report.inputs.every((input: { sha256: string }) => input.sha256.length === 64)).toBe(true);
    expect(report.rows.every((row: { fresh: unknown[]; repeated: { scanMs: number[] } }) =>
      row.fresh.length === 1 && row.repeated.scanMs.length === 1)).toBe(true);
    expect(report.methodology).toContain("not cold filesystem");
    expect(report.metadata.nativeBinarySha256).toHaveLength(64);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}, 30_000);

test("failed benchmarks retain completed cases and mark the report incomplete", async () => {
  const root = await mkdtemp(join(tmpdir(), "badbox-bench-checkpoint-"));
  try {
    const output = join(root, "partial.json");
    await expect(runBenchmark({ output, samples: 1, ruleCounts: [1, 10], progress(message) {
      if (message.startsWith("Measuring corpus/rust/sparse: 10")) throw new Error("interrupted checkpoint test");
    } })).rejects.toThrow("interrupted checkpoint test");
    const report = await Bun.file(output).json();
    expect(report.status).toBe("failed");
    expect(report.error).toContain("interrupted checkpoint test");
    expect(report.rows).toHaveLength(1);
    expect(report.rows[0].rules).toBe(1);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}, 30_000);
