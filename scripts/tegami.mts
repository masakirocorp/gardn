import { tegami } from "tegami";
import { runCli } from "tegami/cli";
import { cargo } from "tegami/plugins/cargo";
import { pathToFileURL } from "node:url";

export function tegamiPrerelease(): "beta" | undefined {
  const value = process.env.GARDN_TEGAMI_PRERELEASE?.trim();
  if (!value) {
    return undefined;
  }
  if (value !== "beta") {
    throw new Error('GARDN_TEGAMI_PRERELEASE must be "beta" when set');
  }
  return value;
}

export function createPaper() {
  const prerelease = tegamiPrerelease();
  return tegami({
    cwd: process.cwd(),
    npm: {
      client: "pnpm",
      updateLockFile: true,
    },
    plugins: [
      cargo({
        updateLockFile: true,
      }),
    ],
    packages: {
      gardn: {
        publish: false,
        ...(prerelease ? { prerelease } : {}),
      },
      "gardn-docs": {
        publish: false,
      },
      "gardn-nix": {
        publish: false,
      },
    },
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  await runCli(createPaper());
}
