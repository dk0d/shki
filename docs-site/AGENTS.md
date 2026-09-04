## Content rules

- Internal doc links MUST be relative (`../../commands/generate/`), never
  root-absolute — archived version copies (starlight-versions) rely on relative
  links to stay inside their version.
- Never edit `src/content/docs/<version>/` or `src/content/versions/` by hand;
  they are release archives created by the release flow (`task docs:version`)
  or backfilled from tags (`task docs:archive-tag`). The unprefixed docs track
  `main` and are served at `/next/`; the site root serves the latest release's
  archive (materialized by `scripts/postbuild.mjs` at build time).
- Svelte components are supported in `.mdx` pages; shadcn-svelte primitives
  live in `src/lib/components/ui/` (add via `bunx shadcn-svelte add <name>`),
  site components in `src/components/`. Tailwind runs without preflight so
  Starlight styles stay intact; use the `not-content` class on component roots
  and rely on `dark:` (wired to `[data-theme]`) for theming.

## Development

When starting the dev server, use background mode:

```
astro dev --background
```

Manage the background server with `astro dev stop`, `astro dev status`, and `astro dev logs`.

## Documentation

Full documentation: https://docs.astro.build

Consult these guides before working on related tasks:

- [Adding pages, dynamic routes, or middleware](https://docs.astro.build/en/guides/routing/)
- [Working with Astro components](https://docs.astro.build/en/basics/astro-components/)
- [Using React, Vue, Svelte, or other framework components](https://docs.astro.build/en/guides/framework-components/)
- [Adding or managing content](https://docs.astro.build/en/guides/content-collections/)
- [Adding styles or using Tailwind](https://docs.astro.build/en/guides/styling/)
- [Supporting multiple languages](https://docs.astro.build/en/guides/internationalization/)
