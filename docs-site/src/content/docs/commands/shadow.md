---
title: shki shadow
description: Manage the embedded Shadow Database server.
---

```bash
shki shadow install <archive>
```

## `install`

Installs embedded PostgreSQL from a release archive you downloaded yourself,
for machines that cannot reach the GitHub API: rate-limited or filtering
corporate proxies, or air-gapped hosts. See
[Embedded PostgreSQL download fails](../../reference/troubleshooting/#embedded-postgresql-download-fails)
for why that happens.

1. Open the [postgresql-binaries releases](https://github.com/theseus-rs/postgresql-binaries/releases)
   page in a browser and download the `postgresql-<version>-<target>.tar.gz` asset for
   the machine, e.g. `postgresql-18.4.0-aarch64-apple-darwin.tar.gz`. The
   target triple is printed in the error if you pick the wrong one.
2. Install it:

   ```bash
   shki shadow install ~/Downloads/postgresql-18.4.0-aarch64-apple-darwin.tar.gz
   ```

The archive is extracted into the same directory the automatic download uses
(`~/.theseus/postgresql/<version>/`), so `diff`, `generate`, `codegen`, and
`queries` find it and make no network request. Install a major version other
than the default `18` with `--pg-version` matching it on those commands.

Re-running `install` for a version that is already present is a no-op.
