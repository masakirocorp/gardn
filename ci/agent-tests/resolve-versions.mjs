#!/usr/bin/env node
// Resolve the current live-agent CLI cohort once for a workflow run.
// stdout: single JSON document. Diagnostics on stderr. Nonzero on failure.
//
// Override knobs (tests / diagnosis):
//   GITHUB_TOKEN | GH_TOKEN     required for GitHub release lookups
//   GARDN_RESOLVE_NPM             npm executable (default: npm)
//   GARDN_RESOLVE_GITHUB_API      API origin (default: https://api.github.com)
//   GARDN_RESOLVE_FETCH           optional path to a node module exporting fetch
//   CLAUDE_CODE_VERSION, CODEX_VERSION, OPENCODE_VERSION, COPILOT_VERSION,
//   HERMES_VERSION, DROID_VERSION, PI_VERSION, KIMI_VERSION, MAKI_VERSION,
//   OMP_REF                     optional exact overrides (skip remote lookup)
//   COHORT_PATH                 optional path to also write the JSON document
//   BUILD_ARGS_PATH             optional path for docker --build-arg lines

import { spawnSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import process from "node:process";

const SCHEMA = 1;

const NPM_AGENTS = [
  {
    name: "claude",
    buildArg: "CLAUDE_CODE_VERSION",
    packageName: "@anthropic-ai/claude-code",
  },
  {
    name: "codex",
    buildArg: "CODEX_VERSION",
    packageName: "@openai/codex",
  },
  {
    name: "opencode",
    buildArg: "OPENCODE_VERSION",
    packageName: "opencode-ai",
  },
  {
    name: "copilot",
    buildArg: "COPILOT_VERSION",
    packageName: "@github/copilot",
  },
  {
    name: "hermes",
    buildArg: "HERMES_VERSION",
    packageName: "hermes-agent",
  },
  {
    name: "droid",
    buildArg: "DROID_VERSION",
    packageName: "droid",
  },
  {
    name: "pi",
    buildArg: "PI_VERSION",
    packageName: "@earendil-works/pi-coding-agent",
  },
];

const KIMI_TAG_PREFIX = "@moonshot-ai/kimi-code@";
const KIMI_ASSETS = [
  "kimi-code-linux-x64.zip",
  "kimi-code-linux-arm64.zip",
  "kimi-code-linux-x64.zip.sha256",
  "kimi-code-linux-arm64.zip.sha256",
];

function fail(message) {
  process.stderr.write(`${message}\n`);
  process.exit(1);
}

function isForbiddenVersionToken(value) {
  const normalized = String(value).trim().toLowerCase();
  return (
    normalized.length === 0 ||
    normalized === "latest" ||
    normalized === "null" ||
    normalized === "undefined" ||
    normalized === "*"
  );
}

// Accept concrete release identifiers only — never dist-tags or wildcards.
function assertVersionShaped(label, value) {
  if (typeof value !== "string") {
    fail(`${label}: expected a string version, got ${typeof value}`);
  }
  const version = value.trim();
  if (isForbiddenVersionToken(version)) {
    fail(`${label}: refused empty or floating version ${JSON.stringify(value)}`);
  }
  // npm/GitHub concrete versions: digits-led semver, optional leading v, or
  // scoped release tags such as @moonshot-ai/kimi-code@1.2.3.
  if (!/^(?:v?\d[\w.+-]*|@[\w.-]+\/[\w.-]+@\d[\w.+-]*)$/i.test(version)) {
    fail(`${label}: version is not concrete: ${JSON.stringify(value)}`);
  }
  return version;
}

function envOverride(name) {
  const raw = process.env[name];
  if (raw === undefined) {
    return undefined;
  }
  const value = String(raw).trim();
  return value.length > 0 ? value : undefined;
}

function resolveNpmVersion(packageName, buildArg) {
  const override = envOverride(buildArg);
  if (override !== undefined) {
    return assertVersionShaped(buildArg, override);
  }

  const npmBin = process.env.GARDN_RESOLVE_NPM || "npm";
  const result = spawnSync(
    npmBin,
    ["view", packageName, "version", "--json"],
    {
      encoding: "utf8",
      env: process.env,
    },
  );

  if (result.error) {
    fail(`npm view ${packageName}: ${result.error.message}`);
  }
  if (result.status !== 0) {
    fail(
      `npm view ${packageName} failed (exit ${result.status}): ${
        result.stderr || result.stdout || ""
      }`.trim(),
    );
  }

  const stdout = (result.stdout || "").trim();
  if (!stdout) {
    fail(`npm view ${packageName}: empty response`);
  }

  let parsed;
  try {
    parsed = JSON.parse(stdout);
  } catch {
    // npm may print a bare version string without JSON quotes when not a TTY.
    parsed = stdout.replace(/^"|"$/g, "");
  }

  if (parsed && typeof parsed === "object" && parsed !== null) {
    if (typeof parsed.version === "string") {
      parsed = parsed.version;
    } else if (typeof parsed.stdout === "string") {
      parsed = parsed.stdout;
    }
  }

  if (typeof parsed !== "string") {
    fail(`npm view ${packageName}: expected version string, got ${stdout}`);
  }

  return assertVersionShaped(buildArg, parsed);
}

function githubToken() {
  const token = envOverride("GH_TOKEN") || envOverride("GITHUB_TOKEN");
  if (!token) {
    fail("GH_TOKEN or GITHUB_TOKEN is required to resolve GitHub releases");
  }
  return token;
}

function githubApiBase() {
  return (process.env.GARDN_RESOLVE_GITHUB_API || "https://api.github.com").replace(
    /\/+$/,
    "",
  );
}

function loadFetch() {
  if (process.env.GARDN_RESOLVE_FETCH) {
    const require = createRequire(import.meta.url);
    const mod = require(process.env.GARDN_RESOLVE_FETCH);
    const candidate = mod.fetch || mod.default || mod;
    if (typeof candidate !== "function") {
      fail(`GARDN_RESOLVE_FETCH must export a fetch function: ${process.env.GARDN_RESOLVE_FETCH}`);
    }
    return candidate.bind(mod);
  }
  if (typeof globalThis.fetch !== "function") {
    fail("global fetch is unavailable");
  }
  return globalThis.fetch.bind(globalThis);
}

async function githubLatestRelease(repo) {
  const token = githubToken();
  const fetchImpl = loadFetch();
  const url = `${githubApiBase()}/repos/${repo}/releases/latest`;
  let response;

  for (let attempt = 1; attempt <= 3; attempt += 1) {
    try {
      response = await fetchImpl(url, {
        headers: {
          Accept: "application/vnd.github+json",
          Authorization: `Bearer ${token}`,
          "User-Agent": "gardn-agent-tests-resolve-versions",
          "X-GitHub-Api-Version": "2022-11-28",
        },
      });
    } catch (error) {
      if (attempt < 3) {
        await new Promise((resolve) => setTimeout(resolve, attempt * 1000));
        continue;
      }
      fail(`GitHub release lookup ${repo}: ${error.message || error}`);
    }

    if (!response || typeof response.status !== "number") {
      fail(`GitHub release lookup ${repo}: invalid response object`);
    }
    if (response.ok) break;

    const body = typeof response.text === "function" ? await response.text() : "";
    const retryable = response.status === 429 || response.status >= 500;
    if (retryable && attempt < 3) {
      await new Promise((resolve) => setTimeout(resolve, attempt * 1000));
      continue;
    }
    fail(
      `GitHub release lookup ${repo}: HTTP ${response.status}${
        body ? `: ${body.slice(0, 400)}` : ""
      }`,
    );
  }

  let payload;
  try {
    payload = await response.json();
  } catch (error) {
    fail(`GitHub release lookup ${repo}: non-JSON body (${error.message || error})`);
  }

  if (payload == null || typeof payload !== "object") {
    fail(`GitHub release lookup ${repo}: null or non-object release payload`);
  }

  const tag = payload.tag_name;
  if (typeof tag !== "string" || isForbiddenVersionToken(tag)) {
    fail(`GitHub release lookup ${repo}: missing or floating tag_name`);
  }

  const assets = Array.isArray(payload.assets) ? payload.assets : [];
  const assetNames = assets
    .map((asset) => (asset && typeof asset.name === "string" ? asset.name : ""))
    .filter(Boolean);

  return { tag: tag.trim(), assetNames, payload };
}

function requireAssets(repo, assetNames, required) {
  const missing = required.filter((name) => !assetNames.includes(name));
  if (missing.length > 0) {
    fail(
      `GitHub release ${repo}: missing required assets: ${missing.join(", ")} (have: ${
        assetNames.join(", ") || "(none)"
      })`,
    );
  }
}

function makiAssetNames(tag) {
  return [
    `maki-${tag}-x86_64-unknown-linux-musl.tar.gz`,
    `maki-${tag}-aarch64-unknown-linux-musl.tar.gz`,
  ];
}

async function resolveKimi() {
  const override = envOverride("KIMI_VERSION");
  if (override !== undefined) {
    const version = assertVersionShaped("KIMI_VERSION", override.replace(/^v/, ""));
    const tag = `${KIMI_TAG_PREFIX}${version}`;
    return {
      name: "kimi",
      buildArg: "KIMI_VERSION",
      entry: {
        source: "github",
        repo: "MoonshotAI/kimi-code",
        tag,
        version,
      },
      buildValue: version,
    };
  }

  const release = await githubLatestRelease("MoonshotAI/kimi-code");
  requireAssets("MoonshotAI/kimi-code", release.assetNames, KIMI_ASSETS);
  const tag = assertVersionShaped("KIMI tag", release.tag);
  if (!tag.startsWith(KIMI_TAG_PREFIX)) {
    fail(`KIMI tag: expected prefix ${KIMI_TAG_PREFIX}, got ${JSON.stringify(tag)}`);
  }
  const version = assertVersionShaped("KIMI_VERSION", tag.slice(KIMI_TAG_PREFIX.length));
  return {
    name: "kimi",
    buildArg: "KIMI_VERSION",
    entry: {
      source: "github",
      repo: "MoonshotAI/kimi-code",
      tag,
      version,
    },
    buildValue: version,
  };
}

async function resolveMaki() {
  const override = envOverride("MAKI_VERSION");
  if (override !== undefined) {
    const raw = assertVersionShaped("MAKI_VERSION", override);
    const tag = raw.startsWith("v") ? raw : `v${raw}`;
    const version = tag.replace(/^v/, "");
    return {
      name: "maki",
      buildArg: "MAKI_VERSION",
      entry: {
        source: "github",
        repo: "tontinton/maki",
        tag,
        version,
      },
      buildValue: tag,
    };
  }

  const release = await githubLatestRelease("tontinton/maki");
  const tag = assertVersionShaped("MAKI tag", release.tag);
  requireAssets("tontinton/maki", release.assetNames, makiAssetNames(tag));
  const version = tag.replace(/^v/, "");
  assertVersionShaped("MAKI_VERSION", version);
  return {
    name: "maki",
    buildArg: "MAKI_VERSION",
    entry: {
      source: "github",
      repo: "tontinton/maki",
      tag,
      version,
    },
    buildValue: tag,
  };
}

async function resolveOmp() {
  const override = envOverride("OMP_REF");
  if (override !== undefined) {
    const tag = assertVersionShaped("OMP_REF", override);
    return {
      name: "omp",
      buildArg: "OMP_REF",
      entry: {
        source: "github",
        repo: "can1357/oh-my-pi",
        tag,
        version: tag.replace(/^v/, ""),
      },
      buildValue: tag,
    };
  }

  const release = await githubLatestRelease("can1357/oh-my-pi");
  const tag = assertVersionShaped("OMP_REF", release.tag);
  requireAssets("can1357/oh-my-pi", release.assetNames, [
    "omp-linux-x64",
    "omp-linux-arm64",
  ]);
  return {
    name: "omp",
    buildArg: "OMP_REF",
    entry: {
      source: "github",
      repo: "can1357/oh-my-pi",
      tag,
      version: tag.replace(/^v/, ""),
    },
    buildValue: tag,
  };
}

function buildArgsLines(buildArgs) {
  return Object.entries(buildArgs)
    .map(([key, value]) => `--build-arg ${key}=${value}`)
    .join("\n");
}

async function main() {
  const agents = {};
  const buildArgs = {};

  for (const agent of NPM_AGENTS) {
    const version = resolveNpmVersion(agent.packageName, agent.buildArg);
    agents[agent.name] = {
      source: "npm",
      package: agent.packageName,
      version,
    };
    buildArgs[agent.buildArg] = version;
  }

  for (const resolved of [await resolveKimi(), await resolveMaki(), await resolveOmp()]) {
    agents[resolved.name] = resolved.entry;
    buildArgs[resolved.buildArg] = resolved.buildValue;
  }

  // Refuse any lingering floating tokens in the concrete build-arg set.
  for (const [key, value] of Object.entries(buildArgs)) {
    assertVersionShaped(key, value);
    if (String(value).toLowerCase() === "latest") {
      fail(`${key}: cohort must not contain latest`);
    }
  }

  const cohort = {
    schema: SCHEMA,
    resolved_at: new Date().toISOString(),
    source: {
      revision: envOverride("SOURCE_REVISION") || envOverride("GITHUB_SHA") || null,
      run_id: envOverride("BUILD_RUN_ID") || envOverride("GITHUB_RUN_ID") || null,
      run_attempt:
        envOverride("BUILD_RUN_ATTEMPT") || envOverride("GITHUB_RUN_ATTEMPT") || null,
    },
    agents,
    build_args: buildArgs,
  };

  const json = `${JSON.stringify(cohort, null, 2)}\n`;
  process.stdout.write(json);

  if (process.env.COHORT_PATH) {
    writeFileSync(process.env.COHORT_PATH, json);
  }
  if (process.env.BUILD_ARGS_PATH) {
    writeFileSync(process.env.BUILD_ARGS_PATH, `${buildArgsLines(buildArgs)}\n`);
  }
}

main().catch((error) => {
  fail(error && error.stack ? error.stack : String(error));
});
