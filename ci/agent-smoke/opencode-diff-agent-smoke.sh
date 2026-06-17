#!/usr/bin/env bash
set -euo pipefail
source /usr/local/lib/hako-agent-smoke-models.sh
primary_model="${HAKO_OPENCODE_SMOKE_MODEL:-openrouter/openrouter/free}"
if [[ -z "${HAKO_SMOKE_ACTIVE_MODEL:-}" ]]; then
  hako_smoke_unique_candidates "$primary_model" "${HAKO_SMOKE_FALLBACK_MODELS:-}" \
    | hako_smoke_opencode_candidates \
    | hako_smoke_run_with_fallbacks "$0" HAKO_OPENCODE_SMOKE_MODEL "$@"
  exit $?
fi

model="$HAKO_SMOKE_ACTIVE_MODEL"
workdir="${HAKO_OPENCODE_DIFF_AGENT_SMOKE_DIR:-$(mktemp -d)}"
mkdir -p "$workdir"


cat > "$workdir/opencode.json" <<EOF_CONFIG
{
  "\$schema": "https://opencode.ai/config.json",
  "model": "$model",
  "small_model": "$model",
  "permission": { "bash": { "*": "deny" } }
}
EOF_CONFIG

cat > "$workdir/prompt.txt" <<'EOF_PROMPT'
You are receiving a Hako native diff payload that was sent to an agent pane.
If the selected hunk changes the Rust function return value from "before" to "after", reply exactly HAKO_DIFF_AGENT_PAYLOAD_OK and nothing else.

Repo: /tmp/hako-diff-agent-smoke
Branch: main
Scope: all
File: src/lib.rs
Bucket: unstaged
Status: modified
Shape: selected hunk

```diff
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn label() -> &'static str {
-    "before"
+    "after"
 }
```
EOF_PROMPT

set +e
timeout "${HAKO_OPENCODE_DIFF_AGENT_SMOKE_TIMEOUT:-180}" opencode run \
  --dir "$workdir" \
  --model "$model" \
  --format default \
  --title hako-diff-agent-payload \
  "$(cat "$workdir/prompt.txt")" >"$workdir/output.txt" 2>&1
status=$?
set -e
if [[ "$status" -ne 0 ]]; then
  if hako_smoke_retryable_status_or_output "$status" "$workdir/output.txt"; then
    echo "opencode diff-agent smoke retryable provider/model failure with $model" >&2
    cat "$workdir/output.txt" >&2
    exit 75
  fi
  echo "opencode diff-agent smoke command failed with status $status" >&2
  cat "$workdir/output.txt" >&2
  exit "$status"
fi

if ! grep -q 'HAKO_DIFF_AGENT_PAYLOAD_OK' "$workdir/output.txt"; then
  echo "opencode diff-agent smoke missing expected marker" >&2
  cat "$workdir/output.txt" >&2
  exit 1
fi

printf 'opencode diff-agent smoke ok: live agent understood hako selected-hunk payload\n'
