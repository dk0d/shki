# `shki` docs site

[Astro Starlight](https://starlight.astro.build) site, deployed to a Dokploy
VPS on release tags (`v*`) by `.github/workflows/docs-deploy.yml`.
`.github/workflows/docs.yml` only build-checks changes on PRs and `main`.

```bash
bun install
bun run dev      # local preview
bun run build    # what CI runs
```

Pages are Markdown under `src/content/docs/`; the sidebar lives in
`astro.config.mjs`. Internal links must be **relative** (`../../commands/...`),
never root-absolute — versioned copies of a page rely on relative links to stay
inside their version.

## Svelte + shadcn-svelte components

`.mdx` pages can embed Svelte components. [shadcn-svelte](https://shadcn-svelte.com)
is set up (lyra preset, Tailwind v4) with components under
`src/lib/components/ui/`; add more with:

```bash
bunx shadcn-svelte@latest add <component> -y --no-deps-install && bun install
```

Site-specific components live in `src/components/` (see
`DialectSupport.svelte` on the landing page, and `VersionSelector.svelte` — the
header version switcher, a shadcn Select fed per-page URLs by the
`ThemeSelect.astro` override). Tailwind is configured
without preflight so Starlight's styles stay intact — wrap component markup in
`not-content` to opt out of Starlight's content styling, and note the `dark:`
variant follows Starlight's `[data-theme]` attribute.

## Versioned docs

Docs are archived per release with
[starlight-versions](https://github.com/HiDeoo/starlight-versions). Authoring
happens in the unprefixed pages under `src/content/docs/` — whatever is on
`main`. The served routes are:

- **`/`** — the latest release's docs (the switcher shows `vX.Y.Z (latest)`)
- **`/next/`** — the in-development docs tracking `main`
- **`/<X.Y.Z>/`** — every archived release

Two build steps produce that layout: `scripts/refresh-next.mjs` re-archives the
working docs as the `next` version before each build, and
`scripts/postbuild.mjs` prunes the root-built pages from `dist/` and symlinks
the latest release's archive entries into their place (relative links, followed
by nginx — nothing is duplicated) — so `dist/` is deploy-ready as-is and nginx
is a plain static file server. Every released version is a static
copy under `src/content/docs/<version>/` (the `next` snapshot is generated and
gitignored).

`versions.json` is the source of truth: `latest` records which archived version
is the most recent release, and `versions` lists all archived releases, newest
first:

```json
{ "latest": "0.10.10", "versions": [{ "slug": "0.10.10", "label": "v0.10.10" }] }
```

Archiving is part of the release flow: when `task patch` / `minor` / `major`
bumps to X.Y.Z, it archives the release's own docs as version X.Y.Z, marks it
as `latest`, and folds the snapshot (`versions.json`,
`src/content/docs/<version>/`, `src/content/versions/`) into the release
commit. Prereleases (`task rc`) are not archived.

To archive out-of-band (e.g. backfilling a historical release), run it
directly with the release tag and commit the result:

```bash
task docs:archive-tag TAG=vX.Y.Z
```

The script is idempotent (an already-archived version is a no-op), refuses
prerelease tags, restores the working docs even on failure, and rewrites the
root-absolute links that pre-`0.10.10` tags used so old archives stay inside
their version.

At build time, `scripts/remark-pin-release-urls.mjs` rewrites
`releases/latest/download` URLs on versioned pages to that version's release
(`releases/download/vX.Y.Z`), so archived install instructions install the
release they document.

### If the docs step of a release fails

`task patch` / `minor` / `major` runs in stages: cargo-release makes the
`release: X.Y.Z` commit first, then the docs archive is created, `latest` is
updated, and both are amended into it. If the docs stage fails (bun/network
trouble, a docs build error), you're left with a release commit that has **no
docs snapshot**. The task exits non-zero, so this doesn't pass silently.

**Caught before pushing** (the normal case) — fix the cause, then re-run the
docs stage and fold it into the same commit:

```bash
task docs:version VERSION=X.Y.Z
(cd docs-site && bun scripts/set-latest.mjs X.Y.Z)
git add docs-site
git commit --amend --no-edit
```

Re-running is safe: an already-archived version no-ops, `set-latest` no-ops
when unchanged, and an empty amend changes nothing. Verify with
`git show --stat HEAD` — the release commit should include
`docs-site/versions.json` and `docs-site/src/content/docs/X.Y.Z/`.

**Noticed after the release merged and tagged** — the site deployed, but the
release is missing from the switcher and `/` still serves the previous one. Don't amend published history;
backfill from the tag on `main` (which also updates `latest` if needed):

```bash
task docs:archive-tag TAG=vX.Y.Z
(cd docs-site && bun scripts/set-latest.mjs X.Y.Z)
git add docs-site
git commit -m "docs: backfill X.Y.Z version archive"
git push
```

The push auto-deploys; **Actions → Docs Deploy → Run workflow** remains as a
manual out-of-band redeploy.

## Deployment (Dokploy)

The site ships as a static nginx image built from `Dockerfile` in this
directory:

1. In Dokploy, create an **Application** from this GitHub repo, branch `main`,
   build type **Dockerfile**, with Build Path / Docker Context `docs-site` and
   Dockerfile Path `docs-site/Dockerfile`.
2. Set the build arg `DOCS_SITE` to the site's canonical URL (used for the
   sitemap and social cards) and attach the docs domain (container port 80).
3. **Enable automatic deploy on push** — `/next/` tracks `main`, and release
   merges carry their version archive and retarget `/`, so every deploy is
   complete.
4. Optionally copy the application's deploy webhook URL into the repo secret
   `DOCS_DEPLOY_HOOK_URL`; `docs-deploy.yml` (manual `workflow_dispatch` only)
   uses it for out-of-band redeploys.
