---
title: Dump A Live Database
description: Export a live database shape as SQL, a JSON Snapshot, or a Directory Schema.
---

Export the live database shape as SQL:

```bash
shki dump
```

Export JSON Snapshot shape:

```bash
shki dump --format json --output snapshot.json
```

Export a Directory Schema:

```bash
shki dump --dirs --output schema
```

Preview Directory Schema output without writing files:

```bash
shki dump --dirs
```

Directory mode writes `main.sql`, top-level `extensions/`, and schema-scoped
object directories where supported.
