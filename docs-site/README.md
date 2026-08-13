# `shki` docs site

[Astro Starlight](https://starlight.astro.build) site published to
<https://dk0d.github.io/shki> by `.github/workflows/docs.yml` on every push to
`main` that touches this directory.

```bash
bun install
bun run dev      # local preview
bun run build    # what CI runs
```

Pages are Markdown under `src/content/docs/`; the sidebar lives in
`astro.config.mjs`.
