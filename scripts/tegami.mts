import { tegami } from "tegami";
import { runCli } from "tegami/cli";
import { cargo } from "tegami/plugins/cargo";

const paper = tegami({
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
    hako: {
      publish: false,
    },
    "hako-docs": {
      publish: false,
    },
    "hako-nix": {
      publish: false,
    },
  },
});

await runCli(paper);
