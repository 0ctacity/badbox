#!/usr/bin/env bun

import { inspect } from "./scanner/index.ts";
import { reportCheck } from "./reporters/terminal.ts";
import { mkdir, writeFile } from "node:fs/promises";
import { dirname, isAbsolute, join } from "node:path";

const usage = "Usage: badbox check [path ...]\n       badbox create <name.badbox>";
const ruleTemplate = `#badbox 1

# Replace or remove this example rule.
rule rust/example-excessive-clones for rust {
  summary "Function contains excessive clone calls"
  param limit = 4
  find code(value) \`value.clone()\`
  group by nearest callable
  when count > limit
  report {
    severity warning
    message "Function contains more clone calls than the configured limit"
    evidence "clone call sites"
  }
}
`;

const [command, ...args] = Bun.argv.slice(2);

if (command === "check" && !args.some((argument) => argument.startsWith("-"))) {
  try {
    const result = await inspect({
      paths: args.length ? args : [process.cwd()],
      rulePaths: [join(process.cwd(), ".badbox")],
    });
    reportCheck(result);
    // Findings are observations, not errors. Incomplete checks are errors.
    if (result.diagnostics.length) process.exitCode = 1;
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
} else if (command === "create" && args.length === 1) {
  const [name] = args;
  const segments = name!.split(/[\\/]/);
  if (!name!.endsWith(".badbox") || isAbsolute(name!) || segments.includes("..")) {
    console.error(usage);
    process.exitCode = 1;
  } else {
    const path = join(process.cwd(), ".badbox", name!);
    try {
      await mkdir(dirname(path), { recursive: true });
      await writeFile(path, ruleTemplate, { flag: "wx" });
      console.log(`Created .badbox/${name!.replaceAll("\\", "/")}`);
    } catch (error) {
      if (error && typeof error === "object" && "code" in error && error.code === "EEXIST") {
        console.error(`Rule file already exists: .badbox/${name!.replaceAll("\\", "/")}`);
      } else {
        console.error(error instanceof Error ? error.message : String(error));
      }
      process.exitCode = 1;
    }
  }
} else {
  console.error(usage);
  process.exitCode = 1;
}
