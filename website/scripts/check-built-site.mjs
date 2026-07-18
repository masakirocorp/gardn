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
