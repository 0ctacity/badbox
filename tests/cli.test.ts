import { expect, test } from "bun:test";
import { fileURLToPath } from "node:url";

const cli = fileURLToPath(new URL("../src/cli.ts", import.meta.url));
const fixtures = fileURLToPath(new URL("./fixtures/counts", import.meta.url));

test("scan runs the native probes and presents observed counts", async () => {
  const proc = Bun.spawn([process.execPath, cli, "scan", fixtures], { stdout: "pipe", stderr: "pipe" });
  const [stdout, stderr, code] = await Promise.all([
    new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited,
  ]);
  expect(code).toBe(0);
  expect(stderr).toBe("");
  expect(stdout).toContain("rust/excessive-clones");
  expect(stdout).toContain("go/excessive-goroutines");
  expect(stdout).toContain("powershell/excessive-invoke-expression");
  expect(stdout).toContain("zig/excessive-as-casts");
  expect(stdout).toContain("2 > 1");
  expect(stdout).toContain("4 files scanned, 9 findings");
});

test("scan rejects unsupported flags instead of silently ignoring them", async () => {
  const proc = Bun.spawn([process.execPath, cli, "scan", "--changed"], { stdout: "pipe", stderr: "pipe" });
  const stderr = await new Response(proc.stderr).text();
  expect(await proc.exited).toBe(1);
  expect(stderr).toContain("Usage:");
});
