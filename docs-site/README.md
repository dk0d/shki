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

During release prep (in the release PR, alongside the `release: X.Y.Z` bump):

```bash
task docs:version VERSION=X.Y.Z
```

This adds the version to `versions.json` and runs a build, which archives the
current docs as that version. Commit everything it generates
(`versions.json`, `src/content/docs/<version>/`, `src/content/versions/`).

At build time, `scripts/remark-pin-release-urls.mjs` rewrites
`releases/latest/download` URLs on versioned pages to that version's release
(`releases/download/vX.Y.Z`), so archived install instructions install the
release they document.

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
   `DOKPLOY_DOCS_WEBHOOK_URL`.

`docs-deploy.yml` then POSTs the webhook whenever a release tag is pushed, so
the site only updates on releases. (The webhook redeploys `main`'s tip, which
by then contains the tagged release and its archived docs; use
`workflow_dispatch` on the workflow for an out-of-band redeploy.)
