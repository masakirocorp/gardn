import { tegami } from "tegami";
import { runCli } from "tegami/cli";
import { cargo } from "tegami/plugins/cargo";

const paper = tegami({
  cwd: process.cwd(),
  plugins: [
    cargo({
      updateLockFile: true,
    }),
  ],
  packages: {
    hako: {
      publish: false,
    },
  },
});

await runCli(paper);
