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

if (result.status === 0) {
  console.log(`
post-check review:
- User-facing behavior: update docs/features.md and the relevant website guide or reference, or record why no docs changed.
- Public contract: update affected API, CLI, config, protocol references, generated schemas, and explicit fixtures.
- Durable architecture decision: add or amend an ADR and its index only when the ADR threshold is met.
- Release-worthy change: add or update the Tegami changefile.`);
}
process.exitCode = result.status ?? 1;
