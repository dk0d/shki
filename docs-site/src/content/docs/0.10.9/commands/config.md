---
title: shki config
description: Print the effective configuration.
slug: 0.10.9/commands/config
---

```bash
shki config
```

Alias: `shki conf`. Prints a key/value table of the configuration actually in
effect after merging, in order: `shki.toml`, environment variables (and `.env`),
then CLI flags.

This is the first command to run when a value isn't what you expect — it shows
the merged result rather than any single source, so it settles questions like
"is it reading my `.env`?" and "which migrations table will this use?".

## Options

Only the global options apply, and they affect the output because they are part
of the merge:

```bash
shki config                              # effective config here
shki config -c db/shki.toml              # for a specific config file
shki config -u postgres://…/other        # what the run would look like with this URL
```

See also: [Configuration reference](../../reference/configuration/).
