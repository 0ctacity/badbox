import { mkdir, copyFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

const root = fileURLToPath(new URL("../", import.meta.url));
const library = {
  darwin: "libbadbox_native.dylib",
  linux: "libbadbox_native.so",
  win32: "badbox_native.dll",
}[process.platform as string];

if (!library) throw new Error(`Unsupported native build platform: ${process.platform}`);

console.log("Building Badbox native engine (release)…");
const build = Bun.spawn([
  "cargo", "build", "--release", "--locked", "--manifest-path", join(root, "native/Cargo.toml"),
  "--target-dir", join(root, "native/target"),
], { cwd: root, stdout: "inherit", stderr: "inherit" });
const code = await build.exited;
if (code !== 0) process.exit(code);
await mkdir(join(root, "native/build"), { recursive: true });
await copyFile(join(root, "native/target/release", library), join(root, "native/build/badbox.node"));
console.log("Native engine built: native/build/badbox.node");
