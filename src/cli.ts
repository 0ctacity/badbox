#!/usr/bin/env bun

const usage = "Usage: badbox scan";

const [command] = Bun.argv.slice(2);

if (command === "scan") {
  console.log("Badbox scan is scaffolded; rule execution is not implemented yet.");
} else {
  console.error(usage);
  process.exitCode = 1;
}
