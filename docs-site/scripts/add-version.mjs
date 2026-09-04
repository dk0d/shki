// Add a release version to versions.json (newest first). The next
// `astro build` archives the current docs under that version — run via
// `task docs:version VERSION=X.Y.Z` so the archive is created and committed
// together with the release.
import { readFileSync, writeFileSync } from "node:fs";

const version = process.argv[2]?.replace(/^v/, "");
if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
  console.error(
    "usage: bun scripts/add-version.mjs <X.Y.Z> (stable versions only — prereleases are not archived)",
  );
  process.exit(1);
}

const path = new URL("../versions.json", import.meta.url);
const versions = JSON.parse(readFileSync(path, "utf8"));

if (versions.some((entry) => entry.slug === version)) {
  console.log(`version ${version} already exists`);
  process.exit(0);
}

versions.unshift({ slug: version, label: `v${version}` });
writeFileSync(path, `${JSON.stringify(versions, null, 2)}\n`);
console.log(`added ${version}; running a build will archive the current docs`);
