import { afterAll, expect, test } from "bun:test";
import { mkdtemp, mkdir, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";

const cli = fileURLToPath(new URL("../src/cli.ts", import.meta.url));
const temporaryRoots: string[] = [];

afterAll(async () => {
  await Promise.all(temporaryRoots.map((root) => rm(root, { recursive: true, force: true })));
});

async function projectWithRules(): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), "badbox-cli-test-"));
  temporaryRoots.push(root);
  await mkdir(join(root, ".badbox"));
  await mkdir(join(root, ".badbox", "nested"));
  await mkdir(join(root, "src"));
  await Bun.write(join(root, ".badbox", "clones.badbox"), `#badbox 1

rule rust/excessive-clones for rust {
  summary "Function contains multiple clone calls"
  find code(value) \`value.clone()\`
  group by nearest callable
  when count > 1
  report {
    severity info
    message "Callable contains too many clone call sites"
    evidence "clone call sites"
  }
}
`);
  await Bun.write(join(root, ".badbox", "nested", "unwraps.badbox"), `#badbox 1

rule rust/excessive-unwraps for rust {
  summary "Function contains multiple unwrap calls"
  find code(value) \`value.unwrap()\`
  group by nearest callable
  when count > 1
  report {
    severity warning
    message "Callable contains too many unwrap call sites"
    evidence "unwrap call sites"
  }
}
`);
  await Bun.write(
    join(root, "src", "sample.rs"),
    "fn example() { a.clone(); b.clone(); a.unwrap(); b.unwrap(); }",
  );
  return root;
}

test("check loads every rule from the project's .badbox directory", async () => {
  const project = await projectWithRules();
  const proc = Bun.spawn([process.execPath, cli, "check"], {
    cwd: project, stdout: "pipe", stderr: "pipe",
  });
  const [stdout, stderr, code] = await Promise.all([
    new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited,
  ]);
  expect(code).toBe(0);
  expect(stderr).toBe("");
  expect(stdout).toContain("rust/excessive-clones");
  expect(stdout).toContain("rust/excessive-unwraps");
  expect(stdout).toContain("2 > 1");
  expect(stdout).toContain("1 files checked, 2 findings");
});

test("check accepts explicit source paths but still gets rules from .badbox", async () => {
  const project = await projectWithRules();
  const proc = Bun.spawn([process.execPath, cli, "check", "src"], {
    cwd: project, stdout: "pipe", stderr: "pipe",
  });
  const stdout = await new Response(proc.stdout).text();
  expect(await proc.exited).toBe(0);
  expect(stdout).toContain("1 files checked, 2 findings");
});

test("check requires a project .badbox directory", async () => {
  const project = await mkdtemp(join(tmpdir(), "badbox-cli-empty-"));
  temporaryRoots.push(project);
  const proc = Bun.spawn([process.execPath, cli, "check"], {
    cwd: project, stdout: "pipe", stderr: "pipe",
  });
  const stderr = await new Response(proc.stderr).text();
  expect(await proc.exited).toBe(1);
  expect(stderr).toContain(".badbox");
});

test("the old scan command and unsupported flags are rejected", async () => {
  const project = await projectWithRules();
  const proc = Bun.spawn([process.execPath, cli, "scan"], {
    cwd: project, stdout: "pipe", stderr: "pipe",
  });
  const stderr = await new Response(proc.stderr).text();
  expect(await proc.exited).toBe(1);
  expect(stderr).toContain("Usage: badbox check");
  expect(stderr).not.toContain("badbox scan");
});

test("create initializes a versioned example rule under .badbox", async () => {
  const project = await mkdtemp(join(tmpdir(), "badbox-cli-create-"));
  temporaryRoots.push(project);
  const proc = Bun.spawn([process.execPath, cli, "create", "rust-rules.badbox"], {
    cwd: project, stdout: "pipe", stderr: "pipe",
  });
  const [stdout, stderr, code] = await Promise.all([
    new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited,
  ]);
  expect(code).toBe(0);
  expect(stderr).toBe("");
  expect(stdout).toContain(".badbox/rust-rules.badbox");
  const source = await Bun.file(join(project, ".badbox", "rust-rules.badbox")).text();
  expect(source.startsWith("#badbox 1\n")).toBe(true);
  expect(source).toContain("rule rust/example-excessive-clones for rust");
});

test("create refuses to overwrite an existing rule file", async () => {
  const project = await mkdtemp(join(tmpdir(), "badbox-cli-existing-"));
  temporaryRoots.push(project);
  await mkdir(join(project, ".badbox"));
  const path = join(project, ".badbox", "rust-rules.badbox");
  await Bun.write(path, "keep me");
  const proc = Bun.spawn([process.execPath, cli, "create", "rust-rules.badbox"], {
    cwd: project, stdout: "pipe", stderr: "pipe",
  });
  const stderr = await new Response(proc.stderr).text();
  expect(await proc.exited).toBe(1);
  expect(stderr).toContain("already exists");
  expect(await Bun.file(path).text()).toBe("keep me");
});
