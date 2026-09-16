#!/usr/bin/env bun

import { inspect } from "./scanner/index.ts";
import { reportScan } from "./reporters/terminal.ts";

const usage = "Usage: badbox scan [path ...]";

const [command, ...paths] = Bun.argv.slice(2);

if (command === "scan" && !paths.some((path) => path.startsWith("-"))) {
  try {
    const result = await inspect({ paths: paths.length ? paths : [process.cwd()] });
    reportScan(result);
    // Findings are observations, not errors. Incomplete scans are errors.
    if (result.diagnostics.length) process.exitCode = 1;
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
} else {
  console.error(usage);
  process.exitCode = 1;
}
