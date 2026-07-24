import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import process from "node:process";

const require = createRequire(import.meta.url);
const turbo = require.resolve("turbo");
const args = process.argv[2] === "--" ? process.argv.slice(3) : process.argv.slice(2);
const result = spawnSync(process.execPath, [turbo, "run", "quality", ...args], {
  env: { ...process.env, CARGO_INCREMENTAL: "0" },
  stdio: "inherit",
});

if (result.error) throw result.error;
process.exitCode = result.status ?? 1;
