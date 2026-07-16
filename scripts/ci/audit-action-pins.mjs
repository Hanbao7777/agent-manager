import { readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";

const workflowDirectory = join(process.cwd(), ".github", "workflows");
const workflowFiles = readdirSync(workflowDirectory)
  .filter((name) => name.endsWith(".yml") || name.endsWith(".yaml"))
  .sort();
const fullShaRef = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+(?:\/[A-Za-z0-9_.-]+)*@[0-9a-fA-F]{40}$/;
const failures = [];
let remoteReferences = 0;
let localReferences = 0;

for (const fileName of workflowFiles) {
  const filePath = join(workflowDirectory, fileName);
  const lines = readFileSync(filePath, "utf8").split(/\r?\n/);

  lines.forEach((line, index) => {
    const match = line.match(/^\s*(?:-\s*)?uses:\s*([^\s#]+)/);
    if (!match) return;

    const reference = match[1];
    if (reference.startsWith("./")) {
      localReferences += 1;
      return;
    }

    remoteReferences += 1;
    if (!fullShaRef.test(reference)) {
      failures.push(`${relative(process.cwd(), filePath)}:${index + 1}: ${reference}`);
    }
  });
}

if (failures.length > 0) {
  console.error("Workflow Action references must use full 40-character commit SHAs:");
  failures.forEach((failure) => console.error(`  ${failure}`));
  process.exitCode = 1;
} else {
  console.log(
    `Verified ${remoteReferences} remote Action references use full commit SHAs; skipped ${localReferences} local references.`,
  );
}
