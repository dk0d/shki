// Make the site root serve the latest release: the root-built pages only
// exist as the archive source for the "next" version, so prune them and
// symlink the latest release's archive entries into their place (relative
// links, preserved by Docker COPY and followed by nginx — no duplicated
// bytes). Runs after every `astro build`, so dist/ is deploy-ready as-is and
// nginx stays a plain static file server.
import { readFileSync, readdirSync, rmSync, symlinkSync } from "node:fs";
import { fileURLToPath } from "node:url";

const dist = new URL("../dist/", import.meta.url);
const { latest } = JSON.parse(
  readFileSync(new URL("../versions.json", import.meta.url), "utf8"),
);

// Keep version dirs, shared assets, and site-level files; prune the rest
// (the pages built from the working docs).
const keep = [
  "_astro",
  "pagefind",
  "next",
  "404.html",
  "favicon.png",
  "robots.txt",
  /^\d+\.\d+\.\d+$/,
  /^sitemap/,
];
for (const entry of readdirSync(dist)) {
  const kept = keep.some((rule) =>
    typeof rule === "string" ? rule === entry : rule.test(entry),
  );
  if (!kept) rmSync(new URL(entry, dist), { recursive: true });
}

for (const entry of readdirSync(new URL(`${latest}/`, dist))) {
  symlinkSync(`${latest}/${entry}`, fileURLToPath(new URL(entry, dist)));
}
console.log(`site root now serves v${latest} (symlinked)`);
