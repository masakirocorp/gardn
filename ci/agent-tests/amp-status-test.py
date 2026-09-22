#!/usr/bin/env python3
"""Exercise the installed Amp CLI and Gardn plugin through a real PTY and RPC socket."""

import fcntl
import json
import os
import pty
import re
import select
import shutil
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import termios
import time
from pathlib import Path


PANE_ID = "amp-status-test"


def check_reports(requests, thread_id):
    if not requests:
        raise RuntimeError("Amp sent no Gardn lifecycle reports")
    sequence = -1
    for request in requests:
        params = request["params"]
        for key, expected in {
            "pane_id": PANE_ID,
            "source": "gardn:amp",
            "agent": "amp",
            "agent_session_id": thread_id,
        }.items():
            if params.get(key) != expected:
                raise RuntimeError(f"Amp report has unexpected {key}: {params.get(key)!r}")
        if params["seq"] <= sequence:
            raise RuntimeError("Amp reports are not in increasing sequence order")
        sequence = params["seq"]
    if requests[0]["method"] != "pane.report_agent_session":
        raise RuntimeError("Amp reported status before selecting its native thread")
    if requests[-1]["method"] != "pane.release_agent":
        raise RuntimeError("Amp did not release its native thread on graceful exit")
    states = [
        request["params"]["state"]
        for request in requests
        if request["method"] == "pane.report_agent"
    ]
    if states != ["idle", "working", "idle"]:
        raise RuntimeError(f"Expected idle -> working -> idle, received {states!r}")


def run_session(server, base, root, env, thread_id, prompt, expected_responses, timeout):
    command = base + [
        "threads", "continue", thread_id, "--no-ide", "--no-notifications",
        "--no-remote-control-terminal",
    ]
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 140, 0, 0))
    process = subprocess.Popen(
        command,
        cwd=root,
        env=env,
        stdin=slave,
        stdout=slave,
        stderr=slave,
        start_new_session=True,
    )
    os.close(slave)
    output = bytearray()
    requests = []
    terminal_open = True

    def pump():
        nonlocal terminal_open
        readers = [server, master] if terminal_open else [server]
        ready, _, _ = select.select(readers, [], [], 0.1)
        if server in ready:
            connection, _ = server.accept()
            with connection:
                connection.settimeout(2)
                data = bytearray()
                while b"\n" not in data:
                    chunk = connection.recv(65536)
                    if not chunk:
                        break
                    data.extend(chunk)
                if data:
                    request = json.loads(data.split(b"\n", 1)[0])
                    requests.append(request)
                    response = {"id": request["id"], "result": {"type": "ok"}}
                    connection.sendall((json.dumps(response) + "\n").encode())
        if master in ready:
            try:
                chunk = os.read(master, 65536)
            except OSError:
                chunk = b""
            if chunk:
                output.extend(chunk)
            else:
                terminal_open = False

    def wait_for(predicate, description):
        deadline = time.monotonic() + timeout
        while not predicate():
            pump()
            if process.poll() is not None and not predicate():
                raise RuntimeError(f"Amp exited {process.returncode} before {description}")
            if time.monotonic() >= deadline:
                raise RuntimeError(f"Timed out waiting for {description}")

    def states():
        return [
            request["params"].get("state")
            for request in requests
            if request["method"] == "pane.report_agent"
        ]

    def response_persisted():
        exported = subprocess.run(
            base + ["threads", "export", thread_id],
            cwd=root, check=True, capture_output=True, text=True, timeout=timeout,
        )
        thread = json.loads(exported.stdout)
        responses = [
            "".join(
                block["text"] for block in message["content"]
                if block["type"] == "text"
            ).strip()
            for message in thread["messages"] if message["role"] == "assistant"
        ]
        if thread["id"] != thread_id:
            raise RuntimeError("Amp exported a different native thread")
        if len(responses) >= len(expected_responses) and responses != expected_responses:
            raise RuntimeError(f"Amp returned unexpected assistant responses: {responses!r}")
        return responses == expected_responses

    try:
        # An existing empty thread gives an observable readiness signal. A fresh TUI
        # has no active thread until its first message, and execute mode has none.
        wait_for(lambda: states() == ["idle"], "the selected thread's initial idle report")
        os.write(master, prompt.encode() + b"\r")
        wait_for(lambda: "working" in states(), "an Amp turn to start")
        wait_for(lambda: states()[-1:] == ["idle"], "the provider turn to complete")
        # Amp can publish idle before the final response reaches its thread store.
        # Read the native transcript, not terminal bytes fragmented by redraws.
        wait_for(response_persisted, "the expected assistant responses to be persisted")
        os.write(master, b"\x03")
        pump()
        os.write(master, b"\x03")
        wait_for(lambda: process.poll() is not None, "Amp to exit gracefully")
        pump()
        if process.returncode != 0:
            raise RuntimeError(f"Amp exited with status {process.returncode}")
        check_reports(requests, thread_id)
    except Exception:
        print(output.decode(errors="replace")[-12000:], file=sys.stderr)
        print(json.dumps(requests, indent=2), file=sys.stderr)
        raise
    finally:
        if process.poll() is None:
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
        # Close the PTY before waiting so pending terminal output cannot block teardown.
        os.close(master)
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=5)


def main():
    repo = Path(os.environ.get("GARDN_REPO_DIR", "/repo"))
    plugin = repo / "apps/gardn/src/integration/assets/amp/gardn-agent-state.ts"
    timeout = float(os.environ.get("GARDN_AMP_STATUS_TIMEOUT", "180"))
    if not plugin.is_file():
        raise RuntimeError(f"Gardn Amp plugin not found: {plugin}; set GARDN_REPO_DIR")

    # Keep the Unix socket path below the macOS/Linux sockaddr_un limit.
    with tempfile.TemporaryDirectory(prefix="gardn-amp-", dir="/tmp") as tmp:
        root = Path(tmp)
        plugins = root / ".amp/plugins"
        plugins.mkdir(parents=True)
        shutil.copyfile(plugin, plugins / plugin.name)
        settings = root / "settings.json"
        settings.write_text(json.dumps({
            "amp.tools.enable": [],
            "amp.updates.mode": "disabled",
            "amp.remoteThreadCreation.enabled": False,
            "amp.skills.disableClaudeCodeSkills": True,
            "amp.skills.disableGlobalAgentsSkills": True,
            "amp.thread.autoArchiveOnQuit": False,
        }))
        base = ["amp", "--settings-file", str(settings), "--log-file", str(root / "amp.log")]
        created = subprocess.run(
            base + ["threads", "new", "--visibility", "private"],
            cwd=root, check=True, capture_output=True, text=True, timeout=timeout,
        )
        match = re.search(r"T-[0-9a-f-]{36}", created.stdout)
        if not match:
            raise RuntimeError(f"Amp did not return a native thread ID: {created.stdout!r}")
        thread_id = match.group()
        try:
            with socket.socket(socket.AF_UNIX) as server:
                endpoint = root / "rpc.sock"
                server.bind(str(endpoint))
                server.listen()
                env = {
                    **os.environ,
                    "GARDN_ENV": "1",
                    "GARDN_SOCKET_PATH": str(endpoint),
                    "GARDN_PANE_ID": PANE_ID,
                    "AMP_SETTINGS_FILE": str(settings),
                    "NO_ANIMATION": "1",
                    "TERM": "xterm-256color",
                }
                turns = [
                    (
                        "provider turn",
                        'Join "GARDN_AMP" and "CI_OK" with an underscore. Reply with only the result. Do not use tools.',
                        "GARDN_AMP_CI_OK",
                    ),
                    (
                        "native resume",
                        "Reply with your previous assistant response followed by _RESUMED. Do not use tools.",
                        "GARDN_AMP_CI_OK_RESUMED",
                    ),
                ]
                expected_responses = []
                for label, prompt, expected in turns:
                    expected_responses.append(expected)
                    run_session(server, base, root, env, thread_id, prompt, expected_responses, timeout)
                    print(f"PASS amp {label}: native thread, idle -> working -> idle, graceful release", flush=True)
        finally:
            subprocess.run(
                base + ["threads", "delete", thread_id],
                cwd=root, check=True, capture_output=True, text=True, timeout=timeout,
            )


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"Amp status test failed: {error}", file=sys.stderr)
        if isinstance(error, subprocess.CalledProcessError) and error.stderr:
            print(error.stderr, file=sys.stderr)
        sys.exit(1)
