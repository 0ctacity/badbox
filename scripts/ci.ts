import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));

const checks = [
  {
    label: "cargo fmt",
    command: ["cargo", "fmt", "--manifest-path", "native/Cargo.toml", "--check"],
  },
  {
    label: "cargo clippy",
    command: [
      "cargo", "clippy", "--manifest-path", "native/Cargo.toml", "--locked", "--all-targets",
      "--", "-D", "warnings",
    ],
  },
  {
    label: "cargo nextest (native)",
    command: ["cargo", "nextest", "run", "--manifest-path", "native/Cargo.toml", "--locked"],
  },
  {
    label: "cargo nextest (tiny-dsl)",
    command: ["cargo", "nextest", "run", "--manifest-path", "native/tiny-dsl/Cargo.toml"],
  },
  {
    label: "bun run build:native",
    command: [process.execPath, "run", "build:native"],
  },
  {
    label: "bun run typecheck",
    command: [process.execPath, "run", "typecheck"],
  },
  {
    label: "bun test",
    command: [process.execPath, "test"],
  },
];

for (const check of checks) {
  console.log(`\n[ci] ${check.label}`);
  const child = Bun.spawn(check.command, {
    cwd: root,
    stdin: "inherit",
    stdout: "inherit",
    stderr: "inherit",
  });
  const exitCode = await child.exited;
  if (exitCode !== 0) process.exit(exitCode);
}

console.log("\n[ci] all checks passed");
