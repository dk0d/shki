// Drop the generated "next" snapshot so the build re-archives the current
// (main-branch) docs as /next/. Runs before every `bun run build`; the
// artifacts are gitignored — next is derived, never committed.
import { rmSync } from "node:fs";

for (const path of [
  "src/content/docs/next",
  "src/assets/next",
  "src/content/versions/next.json",
]) {
  rmSync(new URL(`../${path}`, import.meta.url), {
    recursive: true,
    force: true,
  });
}
