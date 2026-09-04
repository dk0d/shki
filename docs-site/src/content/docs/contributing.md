---
title: Contributing
description: Dev setup, tests, and where the design decisions live.
---

Issues and pull requests are welcome:
[github.com/dk0d/shki](https://github.com/dk0d/shki).

## Setup

```bash
git clone https://github.com/dk0d/shki
cd shki
cargo build
```

Dev tooling is driven by [Task](https://taskfile.dev):

```bash
task tools:install   # cargo-nextest, bacon, cargo-dist, cargo-release, llvm-cov
task up              # start the PostgreSQL container used by integration tests
task down            # stop it and drop volumes
task test            # cargo nextest run
task test:cov:html   # coverage report in the browser
```

## Tests

```bash
cargo nextest run                       # everything
cargo nextest run --test integration    # CLI/end-to-end behavior
cargo nextest run --test querygen       # typed query generation
```

Query codegen fixtures live in `tests/fixtures/querygen/<case>/`. Each
`queries.sql` keeps a `-- ```rust` expectation block immediately above its
query annotation; the default fixture test compares that exact function output
and parses the generated module:

```bash
cargo nextest run --test querygen -E 'test(generated_query_code_is_valid_rust)'
```

The slower Cargo type-check is ignored by default:

```bash
cargo nextest run --test querygen -E 'test(generated_query_code_compiles)' --run-ignored ignored-only
```

## Design decisions

Architecture decision records live in [`docs/adr/`](https://github.com/dk0d/shki/tree/main/docs/adr) — start there before reworking a subsystem.

## Docs site

This site is an [Astro Starlight](https://starlight.astro.build) project under
`docs-site/`, deployed on every push to `main`. The site root serves the
**latest release's** docs; **`/next/`** tracks `main`; each released version is
a static archive reachable from the version switcher, with the newest release
marked `(latest)`.

```bash
cd docs-site
bun install
bun run dev      # local preview
bun run build    # what CI runs
```

Pages are Markdown under `docs-site/src/content/docs/`; the sidebar is defined
in `docs-site/astro.config.mjs`. Internal links must be relative
(`../../commands/...`) so versioned copies stay inside their version.

Docs are versioned per release automatically: `task patch` / `minor` / `major`
archives the release's docs as version X.Y.Z, marks it as the latest release,
and folds the snapshot into the release commit (prereleases are not archived).
See
[`docs-site/README.md`](https://github.com/dk0d/shki/blob/main/docs-site/README.md)
for the details, the deployment setup, and the recovery steps if the docs stage
of a release fails.
