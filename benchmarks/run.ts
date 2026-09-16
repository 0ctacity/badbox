import { createHash } from "node:crypto";
import { copyFile, mkdir, mkdtemp, readdir, rm, writeFile } from "node:fs/promises";
import { arch, cpus, platform, release, tmpdir, totalmem } from "node:os";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { inspect } from "../src/scanner/index.ts";
import { fingerprints, makeRules, measureWorker, summarize, type Density, type Language, type WorkerConfig } from "./harness.ts";

const project = fileURLToPath(new URL("../", import.meta.url));
const corpusRoot = join(project, "tests/fixtures/zova/source");
const sha256 = (value: string | Uint8Array) => createHash("sha256").update(value).digest("hex");
const fullOutput = {
  maxFindings: 0xffffffff,
} as const;

interface SourceInput { path: string; bytes: number; sha256: string }
interface Dataset {
  name: string;
  language: Language;
  paths: string[];
  files: SourceInput[];
  bytes: number;
  sha256: string;
  provenance: string;
}

interface BenchmarkOptions {
  output: string;
  zovaRoot?: string;
  samples?: number;
  ruleCounts?: number[];
  progress?: (message: string) => void;
}

async function command(args: string[], cwd = project): Promise<string> {
  const child = Bun.spawn(args, { cwd, stdout: "pipe", stderr: "pipe" });
  const [stdout, stderr, code] = await Promise.all([
    new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited,
  ]);
  if (code !== 0) throw new Error(`${args.join(" ")}: ${stderr}`);
  return stdout.trimEnd();
}

async function describeInput(name: string, language: Language, root: string, paths: string[], provenance: string): Promise<Dataset> {
  const files = [];
  for (const path of [...paths].sort()) {
    const bytes = new Uint8Array(await Bun.file(path).arrayBuffer());
    files.push({ path: relative(root, path), bytes: bytes.length, sha256: sha256(bytes) });
  }
  return { name, language, paths: [...paths].sort(), files,
    bytes: files.reduce((sum, file) => sum + file.bytes, 0), sha256: sha256(JSON.stringify(files)), provenance };
}

async function datasets(scratch: string, zovaRoot?: string): Promise<Dataset[]> {
  const result: Dataset[] = [];
  for (const language of ["rust", "go"] as const) {
    const extension = language === "rust" ? ".rs" : ".go";
    const paths = (await readdir(corpusRoot)).filter((file) => file.endsWith(extension)).map((file) => join(corpusRoot, file));
    result.push(await describeInput("corpus", language, corpusRoot, paths, "Frozen, independently labeled Zova function excerpts"));
  }
  if (!zovaRoot) return result;
  const root = resolve(zovaRoot);
  const roots = ["bindings/rust/zova/src", "bindings/go"];
  const dirty = await command(["git", "status", "--porcelain", "--", ...roots], root);
  if (dirty) throw new Error("Zova benchmark roots must be clean; commit or choose a clean checkout (no files changed by Badbox)");
  const revision = await command(["git", "rev-parse", "HEAD"], root);
  const tracked = (await command(["git", "ls-files", "-z", "--", ...roots], root)).split("\0")
    .filter((path) => /\.(rs|go)$/.test(path));
  if (!tracked.length) throw new Error("No Rust/Go files found in the expected Zova binding roots");
  const snapshot = join(scratch, "zova");
  for (const path of tracked) {
    await mkdir(dirname(join(snapshot, path)), { recursive: true });
    await copyFile(join(root, path), join(snapshot, path));
  }
  for (const language of ["rust", "go"] as const) {
    const extension = language === "rust" ? ".rs" : ".go";
    const paths = tracked.filter((path) => path.endsWith(extension));
    if (!paths.length) throw new Error(`Zova has no ${language} files`);
    result.push(await describeInput("zova-bindings", language, snapshot, paths.map((path) => join(snapshot, path)), `Zova ${revision}; tracked binding source only`));
    const replicated = join(scratch, "replicated");
    const copies = [];
    for (let replica = 0; replica < 8; replica++) {
      for (const path of paths) {
        const copy = join(replicated, String(replica), path);
        await mkdir(dirname(copy), { recursive: true });
        await copyFile(join(snapshot, path), copy);
        copies.push(copy);
      }
    }
    result.push(await describeInput("zova-replicated-8x", language, replicated, copies,
      `Synthetic 8x file replication of Zova ${revision}; not an independent large repository`));
  }
  return result;
}

async function metadata() {
  const files = ["native/Cargo.lock", "native/Cargo.toml", "native/build.rs", "src/scanner/index.ts", "src/scanner/types.ts",
    ...(await readdir(join(project, "native/src"), { recursive: true }))
      .filter((file) => file.endsWith(".rs")).map((file) => `native/src/${file}`),
    "benchmarks/harness.ts", "benchmarks/run.ts", "benchmarks/worker.ts"];
  const hashes = [];
  for (const path of files.sort()) hashes.push({ path, sha256: sha256(new Uint8Array(await Bun.file(join(project, path)).arrayBuffer())) });
  return {
    timestamp: new Date().toISOString(), bun: Bun.version, rust: await command(["rustc", "--version"]),
    platform: platform(), arch: arch(), osRelease: release(), cpu: cpus()[0]?.model,
    logicalCpus: cpus().length, physicalMemoryBytes: totalmem(),
    badboxRevision: await command(["git", "rev-parse", "HEAD"]),
    workingTreeStatus: await command(["git", "status", "--short"]),
    sourceHashes: hashes,
    nativeBinarySha256: sha256(new Uint8Array(await Bun.file(join(project, "native/build/badbox.node")).arrayBuffer())),
    buildMode: "release required: run bun run build:native before benchmarking",
  };
}

export async function runBenchmark(options: BenchmarkOptions): Promise<void> {
  const samples = options.samples ?? 5;
  // Non-monotonic order avoids always measuring the biggest pack last.
  const ruleCounts = options.ruleCounts ?? [10, 1, 100, 50];
  if (!Number.isInteger(samples) || samples < 1 || samples > 100) throw new Error("samples must be 1..100");
  if (!ruleCounts.length || ruleCounts.some((count) => ![1, 10, 50, 100].includes(count))) throw new Error("unsupported rule counts");
  const progress = options.progress ?? ((message: string) => console.error(message));
  const scratch = await mkdtemp(join(tmpdir(), "badbox-benchmark-"));
  const heartbeat = setInterval(() => progress("Benchmark still running; no samples discarded."), 30_000);
  let recordFailure: ((error: unknown) => Promise<void>) | undefined;
  try {
    const environment = await metadata();
    const inputs = await datasets(scratch, options.zovaRoot);
    const rows: unknown[] = [];
    const report = {
      version: 1, status: "running", error: null as string | null, metadata: environment,
      methodology: "Release native addon; serial benchmark workers; fresh processes, not cold filesystem caches. Findings cross N-API as five-u32 records with interned file/rule tables. Benchmark calls raise maxFindings to u32::MAX so rule-scaling output remains comparable with the historical full-output stress test; normal inspect() calls retain the bounded default. API latency includes rule loading, discovery, parsing, matching, aggregation, compact record construction, result merging, metadata JSON serialization and JS decoding. Process wall time also includes startup, validation and exit. Repeated calls use one warmup and the bounded native rule/parse caches, with no forced GC. One separate worker runs a profiled scan per case; its phase timings and cache/execution counters are recorded under performance but its latency and RSS are excluded from the headline summaries. Peak RSS is per measured worker lifetime, including validation; repeated peak is not per-call memory or leak proof. Reference scans and rule/input preparation are outside measured workers. All samples retained; report min/median/max, not p95 from five samples. No performance acceptance target set. Worker deadline is 60 seconds times the number of warmup/measured calls; completed cases checkpoint between workers.",
      samples, ruleCounts, inputs: inputs.map(({ paths, ...input }) => input), rows,
    };
    await mkdir(dirname(resolve(options.output)), { recursive: true });
    await writeFile(options.output, JSON.stringify(report, null, 2) + "\n", { flag: "wx" });
    const checkpoint = () => writeFile(options.output, JSON.stringify(report, null, 2) + "\n");
    recordFailure = async (error) => {
      report.status = "failed";
      report.error = error instanceof Error ? error.message : String(error);
      await checkpoint();
    };
    for (const [datasetIndex, input] of inputs.entries()) {
      for (const density of ["sparse", "dense"] as Density[]) {
        progress(`Reference: ${input.name}/${input.language}/${density}, ${input.paths.length} files, ${input.bytes} bytes`);
        const ruleDirectory = join(scratch, "rules", String(datasetIndex), density);
        await mkdir(ruleDirectory, { recursive: true });
        const rules = makeRules(input.language, density, 100);
        const rulePaths = [];
        for (const [index, rule] of rules.entries()) {
          const path = join(ruleDirectory, `${index}.yaml`);
          // JSON is a YAML subset; use it to avoid adding a benchmark-only serializer dependency.
          await writeFile(path, JSON.stringify(rule));
          rulePaths.push(path);
        }
        // Untimed single-rule references detect lost/duplicated results when batching.
        // They are not independent ground truth for these benchmark-only selectors.
        const references: Array<string | undefined> = [];
        for (let index = 0; index < 10; index++) {
          const reference = await inspect({
            paths: input.paths,
            rulePaths: [rulePaths[index]!],
            ...fullOutput,
          });
          if (reference.scannedFiles !== input.paths.length) throw new Error("reference did not scan every input file");
          references.push(fingerprints(reference)[rules[index]!.id]);
        }
        for (const count of ruleCounts) {
          progress(`Measuring ${input.name}/${input.language}/${density}: ${count} rules (${samples} fresh + ${samples} repeated calls)`);
          const expected: Record<string, string> = {};
          for (let index = 0; index < count; index++) {
            const signature = references[index % 10];
            if (signature) expected[rules[index]!.id] = signature;
          }
          const config: WorkerConfig = {
            options: { paths: input.paths, rulePaths: rulePaths.slice(0, count), ...fullOutput },
            expected, scannedFiles: input.paths.length, warmups: 0, samples: 1,
          };
          const configPath = join(scratch, "worker.json");
          await writeFile(configPath, JSON.stringify(config));
          const fresh = [];
          for (let trial = 0; trial < samples; trial++) fresh.push(await measureWorker(configPath));
          await writeFile(configPath, JSON.stringify({ ...config, samples, warmups: 1 }));
          const repeated = await measureWorker(configPath);
          await writeFile(configPath, JSON.stringify({ ...config, samples: 1, warmups: 0, profile: true }));
          const profiled = await measureWorker(configPath);
          if (!profiled.performance) throw new Error("profile worker did not return phase timings");
          const medianMs = summarize(repeated.scanMs).median;
          rows.push({
            dataset: input.name, language: input.language, density, rules: count,
            uniqueSelectors: Math.min(count, 10), files: input.paths.length, bytes: input.bytes,
            findings: repeated.findings, findingBytes: repeated.findingBytes,
            fresh, repeated, performance: profiled.performance,
            summary: {
              freshApiMs: summarize(fresh.flatMap((sample) => sample.scanMs)),
              freshProcessMs: summarize(fresh.map((sample) => sample.processWallMs)),
              freshPeakRssBytes: summarize(fresh.map((sample) => sample.peakRssBytes)),
              repeatedApiMs: summarize(repeated.scanMs), repeatedPeakRssBytes: repeated.peakRssBytes,
              filesPerSecond: input.paths.length * 1000 / medianMs,
              mebibytesPerSecond: input.bytes / (1024 * 1024) * 1000 / medianMs,
            },
          });
          await checkpoint();
          progress(`  median repeated ${medianMs.toFixed(2)} ms; peak ${(repeated.peakRssBytes / 1024 / 1024).toFixed(1)} MiB; ${repeated.findings} findings`);
        }
      }
    }
    report.status = "complete";
    await checkpoint();
    progress(`Saved ${rows.length} measured cases to ${options.output}`);
  } catch (error) {
    await recordFailure?.(error);
    throw error;
  } finally {
    clearInterval(heartbeat);
    await rm(scratch, { recursive: true, force: true });
  }
}

if (import.meta.main) {
  const [zovaRoot, requestedOutput, ...extra] = Bun.argv.slice(2);
  if (extra.length) throw new Error("Usage: bun benchmarks/run.ts [zova-checkout] [new-output.json]");
  const output = requestedOutput ?? join(project, "benchmarks/results", `${new Date().toISOString().replaceAll(":", "-")}.json`);
  await runBenchmark({ output, zovaRoot });
}
