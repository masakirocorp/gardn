import { readdir } from "node:fs/promises";

import { createPaper } from "./tegami.mts";

const requiredPackage = process.argv[2] ?? "gardn";
const paper = createPaper();
const draft = await paper.draft();
const changelogs = draft.getChangelogs();
const parsedFilenames = new Set(changelogs.map((entry) => entry.filename));
const markdownFilenames = (await readdir(".tegami")).filter((filename) =>
  filename.endsWith(".md"),
);
const missingRequiredPackage = [
  ...markdownFilenames.filter((filename) => !parsedFilenames.has(filename)),
  ...changelogs
    .filter((entry) => !entry.packages.has(requiredPackage))
    .map((entry) => entry.filename),
];

if (missingRequiredPackage.length > 0) {
  console.error(
    `Every release-worthy Tegami changefile must include ${requiredPackage}. Missing in:`,
  );
  for (const filename of missingRequiredPackage) {
    console.error(`- .tegami/${filename}`);
  }
  process.exit(1);
}

console.log(`tegami release scope ok: all changefiles include ${requiredPackage}`);
