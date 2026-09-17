import { inspect } from "../src/scanner/index.ts";
import { fingerprints, type WorkerConfig } from "./harness.ts";

const path = Bun.argv[2];
if (!path) throw new Error("worker configuration path required");
const config: WorkerConfig = await Bun.file(path).json();
if (!Number.isInteger(config.samples) || config.samples < 1 ||
    !Number.isInteger(config.warmups) || config.warmups < 0) throw new Error("invalid worker sample counts");
const expected = JSON.stringify(Object.entries(config.expected).sort(([a], [b]) => a.localeCompare(b)));
const scanMs: number[] = [];
const warmupMs: number[] = [];
let findings = 0;
let findingBytes = 0;
let phaseProfile;
for (let index = 0; index < config.warmups + config.samples; index++) {
  const started = performance.now();
  const result = await inspect({ ...config.options, profile: config.profile ?? false });
  const elapsed = performance.now() - started;
  (index < config.warmups ? warmupMs : scanMs).push(elapsed);
  // Validation is outside API timing, but inside process wall time and peak RSS.
  if (result.checkedFiles !== config.checkedFiles) throw new Error("checked-file count mismatch");
  const actual = JSON.stringify(Object.entries(fingerprints(result)).sort(([a], [b]) => a.localeCompare(b)));
  if (actual !== expected) throw new Error("result fingerprint mismatch against individual-rule reference");
  findings = result.findingCount;
  findingBytes = result.findings.byteLength;
  phaseProfile = result.performance;
}
if (config.profile && !phaseProfile) throw new Error("native phase profile is unavailable");
console.log(JSON.stringify({ scanMs, warmupMs, findings, findingBytes,
  ...(phaseProfile ? { performance: phaseProfile } : {}) }));
