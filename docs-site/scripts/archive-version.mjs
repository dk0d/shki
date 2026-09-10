// Archive the current working docs as release version X.Y.Z.
//
// starlight-versions archives at most one new version per build, and the
// gitignored `next` snapshot is new on every build (refresh-next.mjs drops
// it), so this runs two builds: one to recreate `next` with the release
// version unregistered, then one that archives only the release. Idempotent:
// an already-archived version is a no-op.
import { execSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";

const version = process.argv[2]?.replace(/^v/, "");
if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
  console.error("usage: bun scripts/archive-version.mjs <X.Y.Z> (stable versions only)");
  process.exit(1);
}

const root = new URL("../", import.meta.url);
const run = (cmd) => execSync(cmd, { cwd: root, stdio: "inherit" });
const archiveDir = new URL(`src/content/docs/${version}/`, root);
if (existsSync(archiveDir)) {
  console.log(`version ${version} is already archived`);
  process.exit(0);
}

// Phase 1: fresh `next`, with the release version unregistered so `next` is
// the only new version (it may already be registered when recovering a
// release whose docs stage failed).
const versionsPath = new URL("versions.json", root);
const original = readFileSync(versionsPath, "utf8");
const data = JSON.parse(original);
data.versions = data.versions.filter((entry) => entry.slug !== version);
writeFileSync(versionsPath, `${JSON.stringify(data, null, 2)}\n`);
try {
  // Not `bun run build`: postbuild.mjs symlinks the latest release's archive,
  // which does not exist yet when the release being archived is latest.
  run("node scripts/refresh-next.mjs && bunx astro build");
} finally {
  writeFileSync(versionsPath, original);
}

// Phase 2: register the version; `next` now exists, so only the release is new.
run(`bun scripts/add-version.mjs ${version}`);
run("bunx astro build && node scripts/postbuild.mjs");
if (!existsSync(archiveDir)) {
  console.error(`archive ${version} was not created by the build`);
  process.exit(1);
}
console.log(`archived ${version}`);
