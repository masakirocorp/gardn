# hako


<p align="center">
  <img src="assets/logo.svg" alt="hako" width="100" />
</p>

<p align="center">
  <a href="https://hako.masakiro.com">hako.masakiro.com</a> · <a href="#install">install</a> · <a href="#quick-start">quick start</a> · <a href="#supported-agents">supported agents</a> · <a href="https://hako.masakiro.com/docs/integrations/">integrations</a> · <a href="https://hako.masakiro.com/docs/configuration/">configuration</a> · <a href="https://hako.masakiro.com/docs/socket-api/">socket api</a>
</p>

---

**agent multiplexer that lives in your terminal.**

workspaces, tabs, panes. mouse-native: click, drag, split. every agent at a glance: blocked, working, done. detach and reattach, agents keep running. no gui app, no electron, no mac-only native wrapper. you see the agent's own terminal, not someone's interpretation of it.

Hako is Masakiro's product fork of [Herdr](https://github.com/ogulcancelik/herdr), originally created by Ogulcan Celik. It uses its own binary, config, sockets, integrations, release channel, and docs so it can coexist with upstream Herdr without namespace collisions.

---

## install

Download a binary from [GitHub releases](https://github.com/masakirocorp/hako/releases) after the first Hako release, or build from source:

```bash
cargo install --path .
```

### update

hako notifies you when a new version is available. run manually to update:

```bash
hako update
```

## quick start

```bash
hako
```

by default hako launches or attaches to one background session server. `ctrl+b q` detaches the client. agents keep running. use `hako server stop` to stop the default server. use `--no-session` for the old single-process mode.

named sessions are runtime/socket namespaces for separate persistent hako servers. they do not replace workspaces; each named session has its own panes, tabs, workspaces, sockets, and session state while sharing the same global config file.

```bash
hako session list
hako session attach work
hako session attach side-project
hako session stop work
hako session delete side-project
```

1. press `ctrl+b`, then `shift+n` to create a workspace
2. run an agent in the root pane
3. press `ctrl+b`, then `w` to open workspace navigation
4. use `ctrl+b`, then `v` or `minus` to split panes, or `ctrl+b`, then `c` to create a new tab
5. watch the sidebar for blocked, working, and done states

on first run hako opens a short onboarding flow. after that, restored sessions land in terminal mode; fresh sessions start in **navigate mode**.

## how it compares

|                          | tmux | gui managers | hako |
|--------------------------|------|--------------|-------|
| persistent sessions       | ✓    | —            | ✓     |
| detach / reattach        | ✓    | —            | ✓     |
| panes, tabs, workspaces  | ✓    | ✓            | ✓     |
| agent awareness          | —    | ✓            | ✓     |
| lives in your terminal   | ✓    | —            | ✓     |
| real terminal views      | ✓    | —            | ✓     |
| mouse-native            | —    | ✓            | ✓     |
| lightweight binary       | ✓    | —            | ✓     |
| agents can orchestrate   | ?    | ?            | ✓     |

tmux gives you persistence and panes, but it was built before agents existed. gui managers show agent state, but they make you leave your terminal and use their wrapped view. hako is persistence and awareness in one tool that stays out of your way.

## persistence

start hako where the work lives. locally, run `hako`. it starts or attaches to the background session automatically, with no socket setup. run your agents, split panes, do your work. press `ctrl+b q` to detach. close your terminal, close your laptop; your agents keep running. open a new terminal, run `hako`, you're back. same session, same panes, same agents.

### from anywhere

need to check on your agents from your phone? just ssh in and run hako. your shell is remote, hako runs there, and the panes keep running there after detach. any ssh client works. no app to download, no account to create.

```
ssh you@yourserver
hako
```

or attach from your local terminal through ssh without opening a shell first. your local hako acts as a thin client, connects over ssh, starts or attaches to the remote hako server, and streams the ui back to your terminal. remote attach uses your local keybindings by default; pass `--remote-keybindings server` to use the remote server config instead.

```bash
hako --remote workbox
hako --remote ssh://you@yourserver:2222
```

for repeat targets, use your ssh config:

```sshconfig
Host workbox
  HostName yourserver
  User you
  Port 2222
```

same session, same agents, same state.

### direct agent attach

`hako` and `hako --remote` attach to the full Hako session UI. `hako agent attach <target>` attaches your current terminal directly to one server-owned terminal, like a single-pane terminal attach. `hako terminal attach <terminal_id>` does the same by terminal id.

Direct attach streams the current rendered terminal state first, then live ANSI frames. Your input goes straight to that terminal. Detach with `ctrl+b q`; send a literal `ctrl+b` with `ctrl+b ctrl+b`. One writable client owns input and resize for a terminal. A second attach fails unless you pass `--takeover`.

## agent awareness

the sidebar shows which agents are blocked, working, or done. workspaces roll up to their most urgent state so you can scan the full list at a glance.

states:

- 🔴 **blocked** — agent needs input or approval
- 🟡 **working** — agent is actively running
- 🔵 **done** — work finished, you have not looked at it yet
- 🟢 **idle** — done and seen

detection works by reading foreground process and terminal output. zero config, no hooks required. for agents that expose hooks, the socket api integration gives more robust state reporting.

## lives in your terminal

not a gui window, not a web dashboard, not electron. hako runs inside whatever terminal you already use. single rust binary, no dependencies. works inside tmux.

## what you get

- **workspaces** — organized around git repos or folder names, each with its own tabs and panes
- **groups** — Arc-style sidebar filters for sets of workspaces inside one session
- **tabs** — first-class in the socket api and cli
- **mouse-native** — click panes/tabs/workspaces/agents, drag borders, select text to copy, right-click menus; not keyboard-only
- **notifications** — sounds and toasts for background events; tab-aware suppression
- **18 built-in themes** — catppuccin, terminal, tokyo night, gruvbox, one, solarized, kanagawa, rosé pine, vesper, and light variants for the main palettes
- **session persistence** — pane processes survive client detach; sessions restore after full restart

## agents can use hako too

the local unix socket lets agents create workspaces, split panes, spawn helpers, read output, and wait for state changes.

```bash
# create a workspace and tab
hako workspace create --cwd ~/project --label "api"
hako tab create --label "logs"

# split a pane and run
hako pane split 1-1 --direction right
hako pane run 1-2 "npm test"

# wait for a pane-level UI attention state
hako wait agent-status 1-1 --status done

# read output
hako pane read 1-2 --source recent --lines 50

# read a rendered ANSI snapshot for TUI feedback loops
hako pane read 1-2 --source visible --ansi
```

full reference: [socket api](https://hako.masakiro.com/docs/socket-api/) and [`SKILL.md`](./SKILL.md).

## supported agents

automatic detection works out of the box. process name matching plus terminal output heuristics.

| agent | idle / done | working | blocked |
|-------|-------------|---------|---------|
| [pi](https://pi.dev) | ✓ | ✓ | partial |
| [claude code](https://docs.anthropic.com/en/docs/claude-code) | ✓ | ✓ | ✓ |
| [codex](https://github.com/openai/codex) | ✓ | ✓ | ✓ |
| [droid](https://factory.ai) | ✓ | ✓ | ✓ |
| [amp](https://ampcode.com) | ✓ | ✓ | ✓ |
| [opencode](https://github.com/anomalyco/opencode) | ✓ | ✓ | ✓ |
| [grok cli](https://x.ai/grok) | ✓ | ✓ | ✓ |
| [hermes agent](https://github.com/NousResearch/hermes-agent) | ✓ | ✓ | ✓ |
| cursor agent | ✓ | ✓ | ✓ |
| antigravity cli | ✓ | ✓ | ✓ |
| kimi code cli | ✓ | ✓ | ✓ |
| [github copilot cli](https://github.com/features/copilot) | ✓ | ✓ | ✓ |
| [kiro cli](https://kiro.dev/docs/cli/) | ✓ | ✓ | — |

detected but not fully tested: gemini cli, cline.

for agents outside the built-in list, hako still works as a terminal multiplexer with workspaces, panes, and tiling. custom integrations can report agent labels over the socket api. see the [socket api docs](https://hako.masakiro.com/docs/socket-api/).

### direct integrations

the built-in pi, omp, claude code, codex, opencode, and hermes integrations forward semantic state to hako over the socket api. install with:

```bash
hako integration install pi
hako integration install omp
hako integration install claude
hako integration install codex
hako integration install opencode
hako integration install hermes
```

see the [integrations docs](https://hako.masakiro.com/docs/integrations/) for setup details.

## keybindings

press `ctrl+b` to enter prefix mode. default actions are prefix-first and tmux-like:

| key | action |
|-----|--------|
| `prefix+c` | new tab |
| `prefix+n` / `prefix+p` | next / previous tab |
| `prefix+1..9` | switch tab |
| `prefix+w` | workspace navigation |
| `prefix+shift+n` | new workspace |
| `prefix+shift+g` | new worktree |
| `prefix+shift+w` | rename workspace |
| `prefix+shift+d` | close workspace |
| `prefix+h/j/k/l` | focus pane |
| `prefix+v` / `prefix+minus` | split pane |
| `prefix+x` | close pane |
| `prefix+b` | toggle sidebar |
| `prefix+z` | zoom pane |
| `prefix+r` | resize mode |
| `prefix+q` | detach |

resize mode: `h`/`l` resize width, `j`/`k` resize height, `esc` exit.

custom command keybindings can launch detached shell helpers or temporary panes:

```toml
[[keys.command]]
key = "prefix+g"
type = "pane" # "shell" or "pane"
command = "lazygit"
```

if you have old custom keybindings and want the new defaults, run `hako config reset-keys`. hako backs up `config.toml`, removes only keybinding config, and uses built-in v2 defaults after restart or config reload.

mouse is supported throughout. full reference: [configuration docs](https://hako.masakiro.com/docs/configuration/).

## configuration

config file: `~/.config/hako/config.toml`

```bash
hako --default-config   # print full default config
```

in-app settings screen for theme, sound, and toast preferences. full reference: [configuration docs](https://hako.masakiro.com/docs/configuration/).

## logs

hako writes logs under `~/.config/hako/`.

common files:

```text
~/.config/hako/hako.log
~/.config/hako/hako-client.log
~/.config/hako/hako-server.log
```

in persistent session mode, the client and server logs are usually the useful ones. logs rotate automatically and keep a few older files like `.1` and `.2`.

for issue reports, include the relevant current log plus rotated siblings if they exist. default logs are metadata-focused and avoid pane contents by default.

use a higher log level only when needed:

```bash
HAKO_LOG=hako=debug hako
```

full logging and environment variable details: [configuration docs](https://hako.masakiro.com/docs/configuration/).

## docs

- [configuration](https://hako.masakiro.com/docs/configuration/) — keybindings, themes, notifications, environment variables
- [integrations](https://hako.masakiro.com/docs/integrations/) — pi, omp, claude code, codex, opencode, hermes integrations
- [`SKILL.md`](./SKILL.md) — reusable agent skill
- [socket api](https://hako.masakiro.com/docs/socket-api/) — socket protocol and cli reference

## agent instructions

If you are an AI agent helping with this repository, read [`AGENTS.md`](./AGENTS.md) before making changes.

## building from source

```bash
git clone https://github.com/masakirocorp/hako
cd hako
cargo build --release
./target/release/hako
```

## nix

Hako provides optional Nix flake outputs for users who already use Nix. The flake builds Hako from source.

```bash
nix run github:masakirocorp/hako/v0.x.y
nix build github:masakirocorp/hako/v0.x.y
nix develop github:masakirocorp/hako/v0.x.y
```

Replace `v0.x.y` with the latest release tag. You can omit the tag to track `master`, but release tags are recommended for normal installs.

The flake exposes `packages.<system>.default`, `apps.<system>.default`, `devShells.<system>.default`, and `overlays.default`.

Update through the same Nix workflow you used to install Hako. For profile installs, run `nix profile list` and then `nix profile upgrade <index-or-name>`. For flake inputs, run `nix flake update hako` in your own flake and rebuild.

## testing

```bash
just test        # unit tests
just check       # formatting, tests, and maintenance checks
```

## license

AGPL-3.0: free to use, modify, and distribute. modified versions must be open-sourced under the same license.

## pi, ghostty, and shift+enter

hako does not require or install terminal keybinds for pi.

ghostty does not ship a default `shift+enter=text:\n` or `shift+enter=text:\x1b\r` keybind. if those lines exist in your ghostty config, they were added by user config or another tool, commonly claude code. they collapse shift+enter into legacy bytes, so downstream programs cannot reliably distinguish shift+enter from ctrl+j or alt+enter.

if shift+enter behaves differently in pi inside hako, first remove those custom terminal keybinds and retest. do not file this as a hako keyboard encoding bug unless it reproduces with a clean terminal config.

related context: #78, #81, #106, and earendil-works/pi#1872.
