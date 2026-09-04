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
[starlight-versions](https://github.com/HiDeoo/starlight-versions). The
unprefixed pages under `src/content/docs/` are always the latest; each released
version is a static copy under `src/content/docs/<version>/`, listed in
`versions.json` (newest first) and switchable from the site header.

Archiving is part of the release flow: `task patch` / `minor` / `major` runs
the version bump, archives the current docs as the new version, and folds the
snapshot (`versions.json`, `src/content/docs/<version>/`,
`src/content/versions/`) into the release commit. Prereleases (`task rc`) are
not archived and do not deploy docs.

To archive out-of-band (e.g. backfilling a historical version), run it
directly and commit the result:

```bash
task docs:version VERSION=X.Y.Z
```

At build time, `scripts/remark-pin-release-urls.mjs` rewrites
`releases/latest/download` URLs on versioned pages to that version's release
(`releases/download/vX.Y.Z`), so archived install instructions install the
release they document.

### If the docs step of a release fails

`task patch` / `minor` / `major` runs in two stages: cargo-release makes the
`release: X.Y.Z` commit first, then the docs archive is created and amended
into it. If the docs stage fails (bun/network trouble, a docs build error),
you're left with a release commit that has **no docs snapshot**. The task exits
non-zero, so this doesn't pass silently.

**Caught before pushing** (the normal case) — fix the cause, then re-run the
docs stage and fold it into the same commit:

```bash
task docs:version VERSION=X.Y.Z
git add docs-site
git commit --amend --no-edit
```

Re-running is safe: if the version is already in `versions.json` the script
no-ops, an existing archive is left alone, and an empty amend changes nothing.
Verify with `git show --stat HEAD` — the release commit should include
`docs-site/versions.json` and `docs-site/src/content/docs/X.Y.Z/`.

**Noticed after the release merged and tagged** — the site deployed, but the
new version is missing from the switcher (the deploy itself is fine; latest
docs are current). Don't amend published history; backfill on `main`:

```bash
task docs:version VERSION=X.Y.Z
git add docs-site
git commit -m "docs: backfill X.Y.Z version archive"
git push
```

then redeploy out-of-band: **Actions → Docs Deploy → Run workflow** (its
`workflow_dispatch` trigger exists for exactly this).

## Deployment (Dokploy)

The site ships as a static nginx image built from `Dockerfile` in this
directory:

1. In Dokploy, create an **Application** from this GitHub repo, branch `main`,
   build type **Dockerfile**, with Build Path / Docker Context `docs-site` and
   Dockerfile Path `docs-site/Dockerfile`.
2. Set the build arg `DOCS_SITE` to the site's canonical URL (used for the
   sitemap and social cards) and attach the docs domain.
3. **Disable automatic deploy on push** — deploys are triggered per release
   tag by GitHub Actions.
4. Copy the application's deploy webhook URL into the repo secret
   `DOCS_DEPLOY_HOOK_URL`.

`docs-deploy.yml` then POSTs the webhook whenever a release tag is pushed, so
the site only updates on releases. (The webhook redeploys `main`'s tip, which
by then contains the tagged release and its archived docs; use
`workflow_dispatch` on the workflow for an out-of-band redeploy.)
