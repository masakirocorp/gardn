# Gardn

<p align="center">
  <img src="apps/gardn/assets/logo.svg" alt="Gardn" width="100" />
</p>

Gardn is a terminal workspace manager for AI coding agents. It combines persistent sessions, workspaces, tabs, panes, mouse-first navigation, and agent-status awareness in one Rust terminal application.

This repository is Masakiro's long-lived product fork of [the upstream `ogulcancelik/herdr` repository](https://github.com/ogulcancelik/herdr). Gardn has its own `gardn` binary, configuration, sockets, integrations, release channel, documentation, and product identity.

## Documentation

- [Public documentation](https://gardn.dev/docs)
- [Installation status](https://gardn.dev/download)
- [Quick start](https://gardn.dev/docs/getting-started/quick-start)
- [Configuration reference](https://gardn.dev/docs/reference/configuration)
- [CLI reference](https://gardn.dev/docs/reference/cli)
- [Local API](https://gardn.dev/docs/api)

The public manual lives under `website/content/**`. Maintainer documentation and architecture decisions remain under `docs/**`.

## Build from source

Public release artifacts are still being verified. Build the current checkout with:

```bash
cargo build --release
./target/release/gardn
```

Or install it from this checkout:

```bash
cargo install --path apps/gardn
gardn --version
```

The crate is not published to crates.io. See the [installation guide](https://gardn.dev/docs/getting-started/install) for supported alternatives and current release status.

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
pnpm --filter @gardn/website dev
```

Maintainer local tooling lives in [`docs/development.md`](docs/development.md), including the Just recipe list. Keep production `gardn` on the latest GitHub release. Use `gardn-dev` for checkout builds. Run `just --list` for the live recipe index.

AI coding agents must read [`AGENTS.md`](./AGENTS.md) before changing this repository.

## License

Gardn is licensed under the [GNU Affero General Public License v3.0](./LICENSE).
