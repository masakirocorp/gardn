import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const websiteRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const publicRoot = path.join(websiteRoot, "dist", "public");

/**
 * @param {string} directory
 * @param {string} prefix
 * @returns {Promise<string[]>}
 */
const listFiles = async (directory, prefix = "") => {
  const entries = await readdir(directory, { withFileTypes: true });
  const nested = await Promise.all(
    entries.map(async (entry) => {
      const relative = path.posix.join(prefix, entry.name);
      return entry.isDirectory()
        ? listFiles(path.join(directory, entry.name), relative)
        : [relative];
    }),
  );
  return nested.flat();
};

const files = await listFiles(publicRoot);
const fileSet = new Set(files);
/** @type {string[]} */
const failures = [];

/**
 * @param {string} label
 * @param {string[]} candidates
 * @returns {string | undefined}
 */
const findFile = (label, candidates) => {
  const found = candidates.find((candidate) => fileSet.has(candidate));
  if (!found) failures.push(`${label}: expected one of ${candidates.join(", ")}`);
  return found;
};

const routes = new Map([
  ["home", ["index.html"]],
  ["download", ["download.html", "download/index.html"]],
  ["releases", ["releases.html", "releases/index.html"]],
  ["docs", ["docs.html", "docs/index.html"]],
  ["docs concepts", ["docs/concepts.html", "docs/concepts/index.html"]],
  [
    "docs install",
    ["docs/getting-started/install.html", "docs/getting-started/install/index.html"],
  ],
  [
    "docs quick start",
    ["docs/getting-started/quick-start.html", "docs/getting-started/quick-start/index.html"],
  ],
  [
    "docs workspace navigation",
    [
      "docs/guides/workspaces-and-navigation.html",
      "docs/guides/workspaces-and-navigation/index.html",
    ],
  ],
  [
    "docs copy and terminal",
    ["docs/guides/copy-and-terminal.html", "docs/guides/copy-and-terminal/index.html"],
  ],
  ["docs remote", ["docs/guides/remote.html", "docs/guides/remote/index.html"]],
  [
    "docs plugins and integrations",
    [
      "docs/guides/plugins-and-integrations.html",
      "docs/guides/plugins-and-integrations/index.html",
    ],
  ],
  [
    "docs updates and handoff",
    ["docs/guides/updates-and-handoff.html", "docs/guides/updates-and-handoff/index.html"],
  ],
  [
    "docs troubleshooting",
    ["docs/guides/troubleshooting.html", "docs/guides/troubleshooting/index.html"],
  ],
  ["docs CLI", ["docs/reference/cli.html", "docs/reference/cli/index.html"]],
  [
    "docs configuration",
    ["docs/reference/configuration.html", "docs/reference/configuration/index.html"],
  ],
  [
    "docs keybindings",
    ["docs/reference/keybindings.html", "docs/reference/keybindings/index.html"],
  ],
  [
    "docs plugin manifest",
    ["docs/reference/plugin-manifest.html", "docs/reference/plugin-manifest/index.html"],
  ],
  ["docs platforms", ["docs/reference/platforms.html", "docs/reference/platforms/index.html"]],
  ["docs Local API", ["docs/api.html", "docs/api/index.html"]],
  ["docs Local API workflow", ["docs/api/workflow.html", "docs/api/workflow/index.html"]],
  ["docs Local API errors", ["docs/api/errors.html", "docs/api/errors/index.html"]],
  [
    "docs Local API requests",
    [
      "docs/api/reference/generated/requests.html",
      "docs/api/reference/generated/requests/index.html",
    ],
  ],
  ["404", ["404.html", "404/index.html"]],
]);

for (const [label, candidates] of routes) {
  const routeFile = findFile(label, candidates);
  if (!routeFile) continue;

  const html = await readFile(path.join(publicRoot, routeFile), "utf8");
  if (!html.includes("Oh My Herdr")) failures.push(`${label}: missing product identity`);
  if (label !== "404" && !html.includes('rel="canonical"')) {
    failures.push(`${label}: missing canonical URL`);
  }
  if (!html.includes('name="description"')) failures.push(`${label}: missing description metadata`);
  if (html.includes("My Page") || html.includes("Hello World")) {
    failures.push(`${label}: generated placeholder content leaked into the build`);
  }
  for (const forbidden of [
    "Hako",
    "--handoff-import",
    "remote-client-bridge",
    "ClientMessage",
    "ServerMessage",
    "HandoffManifest",
    "cargo install oh-my-herdr",
  ]) {
    if (html.includes(forbidden)) failures.push(`${label}: unsupported public claim ${forbidden}`);
  }
}

const generatedSchema = JSON.parse(
  await readFile(path.join(websiteRoot, ".generated", "api", "schema.json"), "utf8"),
);
const versionedSchemaFile = findFile("versioned Local API schema", [
  `api/${generatedSchema.product_version}/schema.json`,
]);
if (versionedSchemaFile) {
  const publishedSchema = await readFile(path.join(publicRoot, versionedSchemaFile), "utf8");
  const sourceSchema = `${JSON.stringify(
    JSON.parse(await readFile(path.join(websiteRoot, ".generated", "api", "schema.json"), "utf8")),
    null,
    2,
  )}\n`;
  if (publishedSchema !== sourceSchema) {
    failures.push("versioned Local API schema does not match the generated source");
  }
  for (const internalName of [
    "ClientMessage",
    "ServerMessage",
    "SemanticFrame",
    "HandoffManifest",
  ]) {
    if (publishedSchema.includes(`"${internalName}"`)) {
      failures.push(`versioned Local API schema exposes internal contract ${internalName}`);
    }
  }
}

findFile("sitemap", ["sitemap.xml"]);
findFile("LLM index", ["llms.txt"]);
findFile("LLM full export", ["llms-full.txt"]);
findFile("search index", ["api/search", "api/search.json", "api/search/index.html"]);
findFile("robots policy", ["robots.txt"]);
findFile("security headers", ["_headers"]);
findFile("redirect rules", ["_redirects"]);
findFile("product logo", ["logo.svg"]);
findFile("favicon", ["favicon.svg"]);

const wranglerJsonc = await readFile(path.join(websiteRoot, "wrangler.jsonc"), "utf8");
const wrangler = JSON.parse(wranglerJsonc.replace(/,\s*([}\]])/g, "$1"));
if ("main" in wrangler) failures.push("wrangler: a Worker runtime entry point is not allowed");
if (wrangler.assets?.directory !== "./dist/public") {
  failures.push("wrangler: assets.directory must be ./dist/public");
}
if (wrangler.assets?.not_found_handling !== "404-page") {
  failures.push("wrangler: custom 404 handling is not configured");
}

if (failures.length > 0) {
  console.error(failures.join("\n"));
  process.exitCode = 1;
} else {
  console.log(`built site ok: ${routes.size} routes and ${files.length} assets`);
}
