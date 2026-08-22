import { tegami } from "tegami";
import { runCli } from "tegami/cli";
import { cargo } from "tegami/plugins/cargo";
import { pathToFileURL } from "node:url";

export function createPaper() {
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
