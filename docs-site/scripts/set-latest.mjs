// Record which release the unprefixed (latest) docs represent. Run by the
// release flow when bumping to a new version; the value feeds the version
// switcher's "Latest (vX.Y.Z)" label.
import { readFileSync, writeFileSync } from "node:fs";

const version = process.argv[2]?.replace(/^v/, "");
if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
  console.error("usage: bun scripts/set-latest.mjs <X.Y.Z> (stable versions only)");
  process.exit(1);
}

const path = new URL("../versions.json", import.meta.url);
const data = JSON.parse(readFileSync(path, "utf8"));
if (data.latest === version) {
  console.log(`latest is already ${version}`);
  process.exit(0);
}
data.latest = version;
writeFileSync(path, `${JSON.stringify(data, null, 2)}\n`);
console.log(`latest is now ${version}`);
