---
name: omh
description: "Control omh from inside it. Manage workspaces and tabs, split panes, spawn agents, read output, and wait for state changes — all via CLI commands that talk to the running omh instance over a local unix socket. Use when running inside omh (OMH_ENV=1)."
---

# Oh My Herdr — agent skill

before using this skill, check that `OMH_ENV=1`. if it is not set to `1`, say you are not running inside an Oh My Herdr-managed pane and stop. do not inspect or control the focused omh pane from outside omh.

you are running inside Oh My Herdr, a terminal-native agent multiplexer. omh gives you workspaces, tabs, and panes — each pane is a real terminal with its own shell, agent, server, or log stream — and you can control all of it from the cli.

this means you can:

- see what other panes and agents are doing
- create tabs for separate subcontexts inside one workspace
- split panes and run commands in them
- start servers, watch logs, and run tests in sibling panes
- wait for specific output before continuing
- wait for another agent to finish
- spawn more agent instances

the `omh` binary is available in your PATH. its workspace, tab, pane, and wait commands talk to the running omh instance over a local unix socket.

if you need the raw protocol or full api reference, read the [socket api docs](https://github.com/masakirocorp/oh-my-herdr/tree/master/docs).

## concepts

**workspaces** are project contexts. each workspace has one or more tabs. unless manually renamed, a workspace's label follows the first tab's root pane — usually the repo name, otherwise the root pane's current folder name.

**tabs** are subcontexts inside a workspace. each tab has one or more panes.

**panes** are terminal splits inside a tab. each pane runs its own process — a shell, an agent, a server, anything.

**agent status** is detected automatically by omh. the api exposes one public field for it:

- `agent_status` — `idle`, `working`, `blocked`, `done`, `unknown`

`done` means the agent finished, but you have not looked at that finished pane yet.

plain shells still exist as panes, but Oh My Herdr's sidebar agent section intentionally focuses on detected agents rather than listing every shell.

**ids** — workspace ids look like `1`, `2`. tab ids look like `1:1`, `1:2`, `2:1`. pane ids look like `1-1`, `1-2`, `2-1`. these are compact public ids for the current live session.

important: ids can compact when tabs, panes, or workspaces are closed. do not treat them as durable ids. re-read ids from `workspace list`, `tab list`, `pane list`, or create/split responses when you need a current id. do not guess that an older `1-3` is still the same pane later.

## discover yourself

see what panes exist and which one is focused:

```bash
omh pane list
```

the focused pane is yours. other panes are your neighbors.

list workspaces:

```bash
omh workspace list
```

list sidebar groups:

```bash
omh group list
```

## tab management

list tabs in the current workspace:

```bash
omh tab list --workspace 1
```

create a new tab:

```bash
omh tab create --workspace 1
```

without `--label`, the new tab keeps the default numbered tab name.

create and name it in one step:

```bash
omh tab create --workspace 1 --label "logs"
```

rename it:

```bash
omh tab rename 1:2 "logs"
```

focus it:

```bash
omh tab focus 1:2
```

close it:

```bash
omh tab close 1:2
```

## read another pane

see what is on another pane's screen:

```bash
omh pane read 1-1 --source recent --lines 50
```

- `--source visible` = current viewport
- `--source recent` = recent scrollback as rendered in the pane
- `--source recent-unwrapped` = recent terminal text with soft wraps joined back together

## split a pane and run a command

split your pane to the right and keep focus on your current pane:

```bash
omh pane split 1-2 --direction right --no-focus
```

that prints json with the new pane nested at `result.pane.pane_id`. parse that value, then run a command in that pane:

```bash
NEW_PANE=$(omh pane split 1-2 --direction right --no-focus | python3 -c 'import sys,json; print(json.load(sys.stdin)["result"]["pane"]["pane_id"])')
omh pane run "$NEW_PANE" "npm run dev"
```

split downward instead:

```bash
omh pane split 1-2 --direction down --no-focus
```

## wait for output

block until specific text appears in a pane. useful for waiting on servers, builds, and tests.

for `--source recent`, matching uses unwrapped recent terminal text, so pane width and soft wrapping do not break matches. `pane read --source recent` still shows the pane as rendered. if you want to inspect the same transcript that the waiter matches, use `pane read --source recent-unwrapped`.

```bash
omh wait output 1-3 --match "ready on port 3000" --timeout 30000
```

with regex:

```bash
omh wait output 1-3 --match "server.*ready" --regex --timeout 30000
```

if it times out, exit code is `1`.

## wait for an agent status

block until another agent reaches a specific status:

```bash
omh wait agent-status 1-1 --status done --timeout 60000
```

use this when you want the same `done` / `idle` distinction the UI shows.

## send text or keys to a pane

send text without pressing Enter:

```bash
omh pane send-text 1-1 "hello from claude"
```

press Enter or other keys:

```bash
omh pane send-keys 1-1 Enter
```

`pane run` sends the text and then a real `Enter` key in one request:

```bash
omh pane run 1-1 "echo hello"
```

## workspace management

groups are sidebar filters inside the current session. they do not create separate omh server sockets.

create and focus a group:

```bash
omh group create "client work"
omh group focus 2
```

create a new workspace:

```bash
omh workspace create --cwd /path/to/project
```

without `--label`, the new workspace keeps the default cwd-based name.

create and name one in one step:

```bash
omh workspace create --cwd /path/to/project --label "api server"
```

create one without focusing it:

```bash
omh workspace create --no-focus
```

focus a workspace:

```bash
omh workspace focus 2
```

move a workspace to a group:

```bash
omh workspace move-to-group 2 1
```

rename:

```bash
omh workspace rename 1 "api server"
```

close:

```bash
omh workspace close 2
```

## close a pane

```bash
omh pane close 1-3
```

## recipes

### run a server and wait until it is ready

```bash
NEW_PANE=$(omh pane split 1-2 --direction right --no-focus | python3 -c 'import sys,json; print(json.load(sys.stdin)["result"]["pane"]["pane_id"])')
omh pane run "$NEW_PANE" "npm run dev"
omh wait output "$NEW_PANE" --match "ready" --timeout 30000
omh pane read "$NEW_PANE" --source recent --lines 20
```

### run tests in a separate pane and inspect the result

```bash
omh pane split 1-2 --direction down --no-focus
omh pane run 1-3 "cargo test"
omh wait output 1-3 --match "test result" --timeout 60000
omh pane read 1-3 --source recent --lines 30
```

### check what another agent is working on

```bash
omh pane list
omh pane read 1-1 --source recent --lines 80
```

### watch another pane robustly

use this pattern when you need to coordinate with a sibling pane:

```bash
# inspect what is already there
omh pane read 1-3 --source recent --lines 40

# wait only for the next output you expect
omh wait output 1-3 --match "ready" --timeout 30000

# if you need to inspect the same transcript the waiter matched,
# read the unwrapped recent text directly
omh pane read 1-3 --source recent-unwrapped --lines 40
```

### spawn a new agent and give it a task

```bash
omh pane split 1-2 --direction right --no-focus
omh pane run 1-3 "claude"
omh wait output 1-3 --match ">" --timeout 15000
omh pane run 1-3 "review the test coverage in apps/omh/src/api/"
```

### coordinate with another agent

```bash
omh wait agent-status 1-1 --status done --timeout 120000
omh pane read 1-1 --source recent --lines 100
```

## notes

- `workspace list`, `workspace create`, `tab list`, `tab create`, `tab get`, `tab focus`, `tab rename`, `tab close`, `pane list`, `pane get`, `pane split`, `wait output`, and `wait agent-status` print json on success.
- `pane read` prints text, not json.
- `pane read --format ansi` or `pane read --ansi` returns a rendered ANSI snapshot for TUI feedback loops.
- `pane read --source recent-unwrapped` is useful when you want to inspect the same unwrapped transcript that `wait output --source recent` matches against.
- `pane send-text`, `pane send-keys`, and `pane run` print nothing on success.
- parse ids from `workspace create`, `tab create`, and `pane split` responses when you need new ids. `workspace create` returns `result.workspace`, `result.tab`, and `result.root_pane`. `tab create` returns `result.tab` and `result.root_pane`. for `pane split`, the new pane id is at `result.pane.pane_id`.
- use `pane read` for current output that already exists. use `wait output` for future output you expect next.
- `--no-focus` on split, tab create, and workspace create keeps your current terminal context focused.
- without `--label`, workspace create keeps cwd-based naming and tab create keeps numbered naming.
- `--label` on tab create and workspace create applies the custom name immediately.
- if you are running inside Oh My Herdr, the `OMH_ENV` environment variable is set to `1`.
