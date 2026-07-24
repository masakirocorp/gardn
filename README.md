# Oh My Herdr

<p align="center">
  <img src="apps/omh/assets/logo.svg" alt="Oh My Herdr" width="100" />
</p>

Oh My Herdr is a terminal workspace manager for AI coding agents. It combines persistent sessions, workspaces, tabs, panes, mouse-first navigation, and agent-status awareness in one Rust terminal application.

This repository is Masakiro's long-lived product fork of [Herdr](https://github.com/ogulcancelik/herdr). Oh My Herdr has its own `omh` binary, configuration, sockets, integrations, release channel, documentation, and product identity.

## Documentation

- [Public documentation](https://ohmyherdr.com/docs)
- [Installation status](https://ohmyherdr.com/download)
- [Quick start](https://ohmyherdr.com/docs/getting-started/quick-start)
- [Configuration reference](https://ohmyherdr.com/docs/reference/configuration)
- [CLI reference](https://ohmyherdr.com/docs/reference/cli)
- [Local API](https://ohmyherdr.com/docs/api)

The public manual lives under `website/content/**`. Maintainer documentation and architecture decisions remain under `docs/**`.

## Build from source

Public release artifacts are still being verified. Build the current checkout with:

```bash
cargo build --release
./target/release/omh
```

Or install it from this checkout:

```bash
cargo install --path apps/omh
omh --version
```

The crate is not published to crates.io. See the [installation guide](https://ohmyherdr.com/docs/getting-started/install) for supported alternatives and current release status.

## Develop

```bash
corepack enable
pnpm install --frozen-lockfile
pnpm test
pnpm check
```

Turborepo discovers the native Cargo workspace and orchestrates Rust and website tasks in one graph. Native Cargo discovery and custom task commands are intentionally enabled while experimental, with Turbo pinned exactly to 2.10.6 so upgrades are explicit. Root pnpm scripts are the canonical build, format, test, lint, and check interface; Just is reserved for imperative release, upstream-sync, vendored-build, and live-agent workflows.

Run the website development server from the same pnpm workspace:

```bash
pnpm install --frozen-lockfile
pnpm --filter @omh/website dev
```

AI coding agents must read [`AGENTS.md`](./AGENTS.md) before changing this repository.

## License

Oh My Herdr is licensed under the [GNU Affero General Public License v3.0](./LICENSE).
