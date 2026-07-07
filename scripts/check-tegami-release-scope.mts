import { createPaper } from "./tegami.mts";

const requiredPackage = process.argv[2] ?? "hako";
const paper = createPaper();
const draft = await paper.draft();
const missingRequiredPackage = draft
  .getChangelogs()
  .filter((entry) => entry.packages.size > 0 && !entry.packages.has(requiredPackage));

if (missingRequiredPackage.length > 0) {
  console.error(
    `Every release-worthy Tegami changefile must include ${requiredPackage}. Missing in:`,
  );
  for (const entry of missingRequiredPackage) {
    console.error(`- .tegami/${entry.filename}`);
  }
  process.exit(1);
}

console.log(`tegami release scope ok: all changefiles include ${requiredPackage}`);
