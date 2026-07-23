import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import path from "node:path";
import process from "node:process";

const require = createRequire(import.meta.url);
const packageDir = path.dirname(require.resolve("@ast-grep/cli/package.json"));
const binaryName = process.platform === "win32" ? "ast-grep.exe" : "ast-grep";
const result = spawnSync(path.join(packageDir, binaryName), process.argv.slice(2), {
  stdio: "inherit",
});

if (result.error) throw result.error;
process.exitCode = result.status ?? 1;
