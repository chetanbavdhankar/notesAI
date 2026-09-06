import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { resolve, dirname } from "node:path";
import { spawn } from "node:child_process";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const env = { ...process.env };
if (existsSync(resolve(root, ".tools/cargo/bin/cargo.exe"))) {
  env.CARGO_HOME = resolve(root, ".tools/cargo");
  env.RUSTUP_HOME = resolve(root, ".tools/rustup");
  const pathKey =
    Object.keys(env).find((k) => k.toLowerCase() === "path") || "PATH";
  env[pathKey] = `${resolve(root, ".tools/cargo/bin")};${env[pathKey] || ""}`;
}
const mode = process.argv[2] || "dev";
await import("./fetch-reader.mjs");
const command = mode === "test" ? "cargo" : process.execPath;
const args =
  mode === "test"
    ? [
        "test",
        "--manifest-path",
        "src-tauri/Cargo.toml",
        "--lib",
        ...process.argv.slice(3),
      ]
    : [
        resolve(root, "node_modules/@tauri-apps/cli/tauri.js"),
        mode,
        ...process.argv.slice(3),
      ];
const child = spawn(command, args, {
  cwd: root,
  env,
  stdio: "inherit",
  windowsHide: true,
});
child.on("error", (e) => {
  console.error(
    `Could not start the desktop toolchain: ${e.message}. Install Rust with the MSVC target and Visual Studio C++ Build Tools.`,
  );
  process.exitCode = 1;
});
child.on("exit", (code) => {
  process.exitCode = code ?? 1;
});
